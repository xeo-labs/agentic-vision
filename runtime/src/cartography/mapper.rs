//! Mapper: orchestrates the entire mapping process.
//!
//! This is the core of the cartography engine. It uses a layered acquisition
//! approach:
//!
//! 1. **Layer 0**: Sitemap + robots.txt + HEAD scan + feed discovery
//! 2. **Layer 1**: HTTP GET sample pages + parse structured data (JSON-LD, OG, meta)
//! 3. **Layer 1.5**: Pattern engine (CSS selectors + regex) on pages with <50% structured data
//! 4. **Layer 2**: API discovery for known domains
//! 5. **Layer 2.5**: Action discovery — HTML forms + JS endpoints + platform templates
//! 6. **Layer 3**: Browser render ONLY for pages where Layers 0-2.5 gave <20% data
//!
//! The browser is a last-resort fallback. For most e-commerce and news sites,
//! Layers 1-2.5 provide sufficient data.

use crate::acquisition::action_discovery::{self, HttpAction};
use crate::acquisition::http_client::HttpClient;
use crate::acquisition::pattern_engine::{self, PatternResult};
use crate::acquisition::structured::{self, StructuredData};
use crate::acquisition::{api_discovery, feed_parser, head_scanner};
use crate::cartography::{
    action_encoder, feature_encoder, page_classifier, robots, sitemap, url_classifier,
};
use crate::extraction::loader::ExtractionLoader;
use crate::map::builder::SiteMapBuilder;
use crate::map::types::*;
use crate::renderer::Renderer;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tracing::{info, warn};

/// Request to map a website.
#[derive(Debug, Clone)]
pub struct MapRequest {
    pub domain: String,
    pub max_nodes: u32,
    pub max_render: u32,
    pub timeout_ms: u64,
    pub respect_robots: bool,
}

/// The Mapper orchestrates the entire site mapping process.
pub struct Mapper {
    renderer: Arc<dyn Renderer>,
    extractor_loader: Arc<ExtractionLoader>,
}

impl Mapper {
    pub fn new(renderer: Arc<dyn Renderer>, extractor_loader: Arc<ExtractionLoader>) -> Self {
        Self {
            renderer,
            extractor_loader,
        }
    }

    /// Map an entire site using the layered acquisition approach. Returns a complete SiteMap.
    pub async fn map(&self, request: MapRequest) -> Result<SiteMap> {
        let start = Instant::now();
        let entry_url = format!("https://{}", request.domain);
        info!(
            "mapping {} (max_nodes={}, max_render={}, layered)",
            request.domain, request.max_nodes, request.max_render
        );

        let http_client = HttpClient::new(request.timeout_ms);

        // Time budgets
        let total_budget = std::time::Duration::from_millis(request.timeout_ms);
        let layer0_budget =
            std::time::Duration::from_millis((request.timeout_ms as f64 * 0.30) as u64);
        let layer1_deadline =
            std::time::Duration::from_millis((request.timeout_ms as f64 * 0.70) as u64);

        // ── Layer 0: Metadata (sitemap + robots + HEAD scan + feeds) ──

        // 0a. Fetch robots.txt
        let robots_rules = self
            .fetch_robots(&request.domain, request.respect_robots, &http_client)
            .await;

        // 0b. Fetch sitemap URLs
        let sitemap_entries = tokio::time::timeout(
            layer0_budget.saturating_sub(start.elapsed()),
            self.fetch_sitemap_urls(&request.domain, &robots_rules, &http_client),
        )
        .await
        .unwrap_or_else(|_| {
            warn!(
                "sitemap fetch timed out after {:.1}s",
                start.elapsed().as_secs_f64()
            );
            Vec::new()
        });

        // Collect all discovered URLs
        let mut all_urls: Vec<String> = sitemap_entries.iter().map(|e| e.url.clone()).collect();
        if !all_urls.contains(&entry_url) {
            all_urls.insert(0, entry_url.clone());
        }

        // 0c. Fetch homepage HTML to discover more URLs + feeds
        let homepage_html = match http_client.get(&entry_url, 10000).await {
            Ok(resp) if resp.status == 200 => {
                let body = resp.body;
                let body_for_parse = body.clone();
                let eu = entry_url.clone();
                let links = tokio::task::spawn_blocking(move || {
                    structured::extract_links_from_html(&body_for_parse, &eu)
                })
                .await
                .unwrap_or_default();
                for link in &links {
                    if !all_urls.contains(link) {
                        all_urls.push(link.clone());
                    }
                }
                info!(
                    "homepage HTTP fetch found {} links for {}",
                    links.len(),
                    request.domain
                );
                Some(body)
            }
            _ => None,
        };

        // 0d. Extract URLs from embedded JS state + <link> tags
        if let Some(ref html) = homepage_html {
            let domain_for_js = request.domain.clone();
            let html_for_js = html.clone();
            let js_urls = tokio::task::spawn_blocking(move || {
                extract_urls_from_page_source(&html_for_js, &domain_for_js)
            })
            .await
            .unwrap_or_default();
            for url in &js_urls {
                if !all_urls.contains(url) {
                    all_urls.push(url.clone());
                }
            }
            if !js_urls.is_empty() {
                info!(
                    "JS state + link tags discovered {} URLs for {}",
                    js_urls.len(),
                    request.domain
                );
            }
        }

        // 0e. Feed discovery (non-blocking, time-bounded)
        if let Some(ref html) = homepage_html {
            if start.elapsed() < layer0_budget {
                let feed_entries =
                    feed_parser::discover_feeds(html, &request.domain, &http_client).await;
                for entry in &feed_entries {
                    if !all_urls.contains(&entry.url) {
                        all_urls.push(entry.url.clone());
                    }
                }
                if !feed_entries.is_empty() {
                    info!("feeds discovered {} URLs", feed_entries.len());
                }
            }
        }

        // 0f. If still very few URLs, try common paths as heuristic fallback
        if all_urls.len() < 10 {
            let common_paths = [
                "/about", "/help", "/products", "/blog", "/news", "/categories",
                "/search", "/contact", "/faq", "/terms", "/privacy", "/sitemap",
            ];
            for path in &common_paths {
                let url = format!("https://{}{}", request.domain, path);
                if !all_urls.contains(&url) {
                    all_urls.push(url);
                }
            }
            // Also try www. prefix homepage
            let www_url = format!("https://www.{}", request.domain);
            if !all_urls.contains(&www_url) {
                all_urls.insert(1, www_url);
            }
            info!(
                "few URLs discovered, added common paths (now {} total)",
                all_urls.len()
            );
        }

        // Limit to max_nodes
        let effective_max = (request.max_nodes as usize).min(5000);
        all_urls.truncate(effective_max);

        if all_urls.is_empty() {
            return Err(anyhow::anyhow!("no URLs discovered for {}", request.domain));
        }

        info!(
            "Layer 0 complete: {} URLs discovered in {:.1}s",
            all_urls.len(),
            start.elapsed().as_secs_f64()
        );

        // 0e. HEAD scan to filter HTML pages
        let html_urls = if all_urls.len() > 50 {
            // Only HEAD scan a sample for large sites
            let sample: Vec<String> = all_urls.iter().take(200).cloned().collect();
            let head_results = head_scanner::scan_heads(&sample, &http_client).await;
            let html_only = head_scanner::filter_html_urls(&head_results);
            if html_only.is_empty() {
                all_urls.iter().take(50).cloned().collect()
            } else {
                html_only
            }
        } else {
            all_urls.clone()
        };

        // ── Layer 1: HTTP GET + Structured Data Extraction ──

        // Select sample pages for GET (cap at max_render or 30)
        let sample_count = (request.max_render as usize).min(30).min(html_urls.len());
        let sample_urls: Vec<String> =
            select_diverse_samples(&html_urls, &all_urls, &request.domain, sample_count);

        info!(
            "Layer 1: fetching {} sample pages via HTTP",
            sample_urls.len()
        );

        let responses = http_client.get_many(&sample_urls, 20, 10000).await;

        // Collect successful responses
        let ok_responses: Vec<crate::acquisition::http_client::HttpResponse> = responses
            .into_iter()
            .flatten()
            .filter(|resp| resp.status == 200)
            .collect();

        // Parse structured data + pattern extraction in a blocking task (scraper types are not Send)
        let structured_results = tokio::task::spawn_blocking(move || {
            let mut results: Vec<FetchResult> = Vec::new();
            let mut extra_links: Vec<String> = Vec::new();

            for resp in ok_responses {
                let sd = structured::extract_structured_data(&resp.body, &resp.final_url);

                for link in &sd.links {
                    if link.is_internal {
                        extra_links.push(link.href.clone());
                    }
                }

                let head = crate::acquisition::http_client::HeadResponse {
                    url: resp.url.clone(),
                    status: resp.status,
                    content_type: resp
                        .headers
                        .iter()
                        .find(|(k, _)| k == "content-type")
                        .map(|(_, v)| v.clone()),
                    content_language: resp
                        .headers
                        .iter()
                        .find(|(k, _)| k == "content-language")
                        .map(|(_, v)| v.clone()),
                    last_modified: resp
                        .headers
                        .iter()
                        .find(|(k, _)| k == "last-modified")
                        .map(|(_, v)| v.clone()),
                    cache_control: resp
                        .headers
                        .iter()
                        .find(|(k, _)| k == "cache-control")
                        .map(|(_, v)| v.clone()),
                };

                // Layer 1.5: Run pattern engine on pages with <50% structured data completeness
                let sd_completeness = structured::data_completeness(&sd);
                let pattern_result = if sd_completeness < 0.5 {
                    Some(pattern_engine::extract_from_patterns(
                        &resp.body,
                        &resp.final_url,
                    ))
                } else {
                    None
                };

                // Layer 2.5: Action discovery — forms + platform templates
                let mut http_actions =
                    action_discovery::discover_actions_from_html(&resp.body, &resp.final_url);
                let platform_actions =
                    action_discovery::discover_actions_from_platform(&resp.final_url, &resp.body);
                http_actions.extend(platform_actions);

                results.push((
                    resp.final_url,
                    sd,
                    Some(head),
                    pattern_result,
                    resp.body,
                    http_actions,
                ));
            }

            (results, extra_links)
        })
        .await
        .unwrap_or_default();

        let (structured_results, extra_links) = structured_results;

        // Add discovered links from structured data
        for link in &extra_links {
            if !all_urls.contains(link) {
                all_urls.push(link.clone());
            }
        }

        let pattern_count = structured_results
            .iter()
            .filter(|(_, _, _, pr, _, _)| pr.is_some())
            .count();
        let action_count: usize = structured_results
            .iter()
            .map(|(_, _, _, _, _, actions)| actions.len())
            .sum();
        info!(
            "Layers 1+1.5+2.5 complete: {} pages parsed ({} with pattern fallback, {} HTTP actions) in {:.1}s",
            structured_results.len(),
            pattern_count,
            action_count,
            start.elapsed().as_secs_f64()
        );

        // ── Layer 2: API Discovery ──

        if api_discovery::has_known_api(&request.domain) && start.elapsed() < layer1_deadline {
            let api_urls: Vec<String> = sample_urls.iter().take(5).cloned().collect();
            if let Some(records) =
                api_discovery::try_api(&request.domain, &api_urls, &http_client).await
            {
                info!("Layer 2: API returned {} records", records.len());
                // API data enriches existing structured data but doesn't replace it
            }
        }

        // ── Layer 3: Browser fallback (only for pages with <20% completeness after all layers) ──

        let needs_browser: Vec<String> = structured_results
            .iter()
            .filter(|(_, sd, _, pr, _, _)| {
                let sd_completeness = structured::data_completeness(sd);
                let has_pattern_data = pr
                    .as_ref()
                    .map(|p| {
                        p.price.is_some()
                            || p.rating.is_some()
                            || p.availability.is_some()
                            || p.page_type.is_some()
                    })
                    .unwrap_or(false);
                // Only browser if BOTH structured AND patterns gave <20%
                sd_completeness < 0.2 && !has_pattern_data
            })
            .map(|(url, _, _, _, _, _)| url.clone())
            .collect();

        let mut browser_pages: Vec<BrowserRenderedPage> = Vec::new();

        if !needs_browser.is_empty() && start.elapsed() < total_budget {
            let browser_count = needs_browser.len().min(request.max_render as usize).min(10);
            info!(
                "Layer 3: {} pages need browser fallback (of {} with low completeness)",
                browser_count,
                needs_browser.len()
            );

            for url in needs_browser.iter().take(browser_count) {
                if start.elapsed() >= total_budget {
                    break;
                }
                match self.render_page(url).await {
                    Ok(page) => browser_pages.push(page),
                    Err(e) => warn!("browser fallback failed for {url}: {e}"),
                }
            }

            if !browser_pages.is_empty() {
                info!(
                    "Layer 3 complete: {} pages rendered in {:.1}s",
                    browser_pages.len(),
                    start.elapsed().as_secs_f64()
                );
            }
        }

        // ── Build the map from all layers ──

        // Convert structured_results to the format build_map_from_layers expects
        let layer_results: Vec<LayerResult> = structured_results
            .into_iter()
            .map(|(url, sd, head, pr, _html, actions)| (url, sd, head, pr, actions))
            .collect();

        self.build_map_from_layers(
            &request.domain,
            &all_urls,
            &layer_results,
            &browser_pages,
            request.max_nodes,
        )
    }

    async fn fetch_robots(
        &self,
        domain: &str,
        respect_robots: bool,
        http_client: &HttpClient,
    ) -> Option<robots::RobotsRules> {
        if !respect_robots {
            return None;
        }

        let url = format!("https://{domain}/robots.txt");
        let resp = http_client.get(&url, 5000).await.ok()?;
        if resp.status == 200 {
            Some(robots::parse_robots(&resp.body, "cortex"))
        } else {
            None
        }
    }

    async fn fetch_sitemap_urls(
        &self,
        domain: &str,
        robots_rules: &Option<robots::RobotsRules>,
        http_client: &HttpClient,
    ) -> Vec<sitemap::SitemapEntry> {
        let mut sitemap_urls = Vec::new();

        if let Some(rules) = robots_rules {
            sitemap_urls.extend(rules.sitemaps.clone());
        }

        // Standard paths + common variants
        let candidates = [
            format!("https://{domain}/sitemap.xml"),
            format!("https://{domain}/sitemap_index.xml"),
            format!("https://www.{domain}/sitemap.xml"),
            format!("https://{domain}/sitemaps.xml"),
        ];
        for candidate in &candidates {
            if !sitemap_urls.contains(candidate) {
                sitemap_urls.push(candidate.clone());
            }
        }

        let mut all_entries = Vec::new();
        for url in &sitemap_urls {
            if let Ok(resp) = http_client.get(url, 8000).await {
                if resp.status == 200 {
                    if let Ok(entries) = sitemap::parse_sitemap(&resp.body) {
                        all_entries.extend(entries);
                        if all_entries.len() >= 500 {
                            break; // Enough URLs found
                        }
                    }
                }
            }
        }

        all_entries
    }

    /// Render a single page via browser (Layer 3 fallback).
    async fn render_page(&self, url: &str) -> Result<BrowserRenderedPage> {
        let mut context = self
            .renderer
            .new_context()
            .await
            .context("failed to create browser context")?;

        let nav_result = context
            .navigate(url, 15000)
            .await
            .context("browser navigation failed")?;

        let extraction = self
            .extractor_loader
            .inject_and_run(context.as_ref())
            .await
            .context("browser extraction failed")?;

        let discovered_links: Vec<String> = extraction
            .navigation
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|n| n.get("url").and_then(|u| u.as_str()).map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let _ = context.close().await;

        Ok(BrowserRenderedPage {
            url: url.to_string(),
            final_url: nav_result.final_url.clone(),
            extraction,
            nav_result,
            discovered_links,
        })
    }

    /// Build the final SiteMap from all layers of data.
    fn build_map_from_layers(
        &self,
        domain: &str,
        all_urls: &[String],
        structured_results: &[LayerResult],
        browser_pages: &[BrowserRenderedPage],
        max_nodes: u32,
    ) -> Result<SiteMap> {
        let mut builder = SiteMapBuilder::new(domain);
        let mut url_to_index: HashMap<String, u32> = HashMap::new();

        // Build lookup for browser pages by URL
        let browser_by_url: HashMap<&str, &BrowserRenderedPage> = browser_pages
            .iter()
            .map(|p| (p.url.as_str(), p))
            .chain(browser_pages.iter().map(|p| (p.final_url.as_str(), p)))
            .collect();

        // First pass: add nodes with structured data (Layer 1) or browser data (Layer 3)
        for (url, sd, head, pr, http_actions) in structured_results {
            if url_to_index.len() as u32 >= max_nodes {
                break;
            }

            // Check if we also have browser data for this URL
            if let Some(page) = browser_by_url.get(url.as_str()) {
                // Use browser data (higher quality)
                let (page_type, confidence) = page_classifier::classify_page(&page.extraction, url);
                let encode_result = feature_encoder::encode_features_with_flags(
                    &page.extraction,
                    &page.nav_result,
                    url,
                    page_type,
                    confidence,
                );
                let idx = builder.add_node(
                    url,
                    page_type,
                    encode_result.features,
                    (confidence * 255.0) as u8,
                );
                url_to_index.insert(url.clone(), idx);
                builder.merge_flags(idx, encode_result.flags);
                builder.set_rendered(idx, encode_result.features);

                let actions = action_encoder::encode_actions_from_json(&page.extraction.actions);
                for action in actions {
                    builder.add_action(
                        idx,
                        action.opcode,
                        action.target_node,
                        action.cost_hint,
                        action.risk,
                    );
                }
            } else {
                // Use structured data (Layer 1) + pattern data (Layer 1.5) if available
                let (sd_page_type, sd_confidence) = sd
                    .page_type
                    .unwrap_or_else(|| url_classifier::classify_url(url, domain));

                let default_head = crate::acquisition::http_client::HeadResponse {
                    url: url.clone(),
                    status: 200,
                    content_type: None,
                    content_language: None,
                    last_modified: None,
                    cache_control: None,
                };
                let head_ref = head.as_ref().unwrap_or(&default_head);

                let sd_features =
                    feature_encoder::encode_features_from_structured_data(sd, url, head_ref);

                // Merge with pattern features if available
                let features = if let Some(pattern_result) = pr {
                    let pattern_features = feature_encoder::encode_features_from_patterns(
                        pattern_result,
                        url,
                        head_ref,
                    );
                    let sd_completeness = structured::data_completeness(sd);
                    // Pattern completeness: rough estimate from filled dimensions
                    let pattern_completeness =
                        pattern_features.iter().filter(|&&v| v != 0.0).count() as f32
                            / FEATURE_DIM as f32;
                    feature_encoder::merge_features(
                        &sd_features,
                        sd_completeness,
                        &pattern_features,
                        pattern_completeness,
                        None,
                    )
                } else {
                    sd_features
                };

                // Use pattern page type if higher confidence than structured
                let (final_page_type, final_confidence) = if let Some(pattern_result) = pr {
                    if let Some((pt, pc)) = pattern_result.page_type {
                        if pc > sd_confidence {
                            (pt, pc)
                        } else {
                            (sd_page_type, sd_confidence)
                        }
                    } else {
                        (sd_page_type, sd_confidence)
                    }
                } else {
                    (sd_page_type, sd_confidence)
                };

                let idx = builder.add_node(
                    url,
                    final_page_type,
                    features,
                    (final_confidence * 255.0) as u8,
                );
                url_to_index.insert(url.clone(), idx);

                // Set flags based on structured data + pattern data
                let mut flag_bits: u8 = 0;
                let has_sd_price =
                    !sd.products.is_empty() && sd.products.first().and_then(|p| p.price).is_some();
                let has_pattern_price = pr.as_ref().is_some_and(|p| p.price.is_some());
                if has_sd_price || has_pattern_price {
                    flag_bits |= NodeFlags::HAS_PRICE;
                }
                let has_sd_form = !sd.forms.is_empty();
                let has_pattern_form = pr.as_ref().is_some_and(|p| !p.forms.is_empty());
                if has_sd_form || has_pattern_form {
                    flag_bits |= NodeFlags::HAS_FORM;
                }
                if sd.og.image.is_some() {
                    flag_bits |= NodeFlags::HAS_MEDIA;
                }
                builder.merge_flags(idx, NodeFlags(flag_bits));
            }

            // Wire HTTP-executable actions from Layer 2.5 (action discovery)
            if let Some(&idx) = url_to_index.get(url.as_str()) {
                for action in http_actions {
                    let risk = ((1.0 - action.confidence) * 3.0).min(3.0) as u8;
                    builder.add_action_http(idx, action.opcode, -2, 0, risk);
                }
            }
        }

        // Add browser-only pages (pages rendered but not in structured_results)
        for page in browser_pages {
            if url_to_index.contains_key(&page.url) || url_to_index.contains_key(&page.final_url) {
                continue;
            }
            if url_to_index.len() as u32 >= max_nodes {
                break;
            }

            let (page_type, confidence) =
                page_classifier::classify_page(&page.extraction, &page.url);
            let encode_result = feature_encoder::encode_features_with_flags(
                &page.extraction,
                &page.nav_result,
                &page.url,
                page_type,
                confidence,
            );
            let idx = builder.add_node(
                &page.url,
                page_type,
                encode_result.features,
                (confidence * 255.0) as u8,
            );
            url_to_index.insert(page.url.clone(), idx);
            url_to_index.insert(page.final_url.clone(), idx);
            builder.merge_flags(idx, encode_result.flags);
            builder.set_rendered(idx, encode_result.features);
        }

        // Second pass: add unrendered/un-fetched nodes from URL classification
        for url in all_urls {
            if url_to_index.contains_key(url) {
                continue;
            }
            if url_to_index.len() as u32 >= max_nodes {
                break;
            }

            let (page_type, confidence) = url_classifier::classify_url(url, domain);

            // Default features with basic identity info
            let mut features = [0.0f32; FEATURE_DIM];
            features[FEAT_PAGE_TYPE] = (page_type as u8) as f32 / 31.0;
            features[FEAT_PAGE_TYPE_CONFIDENCE] = confidence;
            features[FEAT_IS_HTTPS] = if url.starts_with("https://") {
                1.0
            } else {
                0.0
            };
            features[FEAT_TLS_VALID] = features[FEAT_IS_HTTPS];

            let idx = builder.add_node(url, page_type, features, (confidence * 255.0) as u8);
            url_to_index.insert(url.clone(), idx);
        }

        // Add edges from structured data links
        for (url, sd, _, _, _) in structured_results {
            let from_idx = match url_to_index.get(url.as_str()) {
                Some(&idx) => idx,
                None => continue,
            };

            for link in &sd.links {
                if link.is_internal {
                    if let Some(&to_idx) = url_to_index.get(&link.href) {
                        if from_idx != to_idx {
                            builder.add_edge(
                                from_idx,
                                to_idx,
                                EdgeType::ContentLink,
                                1,
                                EdgeFlags::default(),
                            );
                            builder.add_edge(
                                to_idx,
                                from_idx,
                                EdgeType::ContentLink,
                                2,
                                EdgeFlags::default(),
                            );
                        }
                    }
                }
            }

            // Add breadcrumb edges
            let mut prev_idx: Option<u32> = None;
            for crumb in &sd.breadcrumbs {
                if let Some(ref crumb_url) = crumb.url {
                    if let Some(&crumb_idx) = url_to_index.get(crumb_url.as_str()) {
                        if let Some(prev) = prev_idx {
                            if prev != crumb_idx {
                                builder.add_edge(
                                    prev,
                                    crumb_idx,
                                    EdgeType::Breadcrumb,
                                    1,
                                    EdgeFlags::default(),
                                );
                            }
                        }
                        prev_idx = Some(crumb_idx);
                    }
                }
            }
        }

        // Add edges from browser pages
        for page in browser_pages {
            let from_idx = match url_to_index
                .get(&page.url)
                .or_else(|| url_to_index.get(&page.final_url))
            {
                Some(&idx) => idx,
                None => continue,
            };

            for link in &page.discovered_links {
                if let Some(&to_idx) = url_to_index.get(link) {
                    if from_idx != to_idx {
                        builder.add_edge(
                            from_idx,
                            to_idx,
                            EdgeType::ContentLink,
                            1,
                            EdgeFlags::default(),
                        );
                        builder.add_edge(
                            to_idx,
                            from_idx,
                            EdgeType::ContentLink,
                            2,
                            EdgeFlags::default(),
                        );
                    }
                }
            }
        }

        // Infer edges from URL structure for all classified URLs
        let classified: Vec<(String, PageType, f32)> = all_urls
            .iter()
            .filter_map(|url| {
                url_to_index.get(url).map(|_| {
                    let (pt, c) = url_classifier::classify_url(url, domain);
                    (url.clone(), pt, c)
                })
            })
            .collect();

        let rendered_set: HashMap<String, usize> = HashMap::new();
        infer_edges_from_url_structure(&classified, &url_to_index, &rendered_set, &mut builder);

        info!("built layered map: {} nodes", url_to_index.len());

        Ok(builder.build())
    }
}

/// Intermediate result from HTTP fetch + structured data + pattern extraction + actions.
type FetchResult = (
    String,
    StructuredData,
    Option<crate::acquisition::http_client::HeadResponse>,
    Option<PatternResult>,
    String,          // raw HTML body
    Vec<HttpAction>, // discovered HTTP-executable actions
);

/// Result passed to the map builder: structured data + patterns + actions (no raw HTML).
type LayerResult = (
    String,
    StructuredData,
    Option<crate::acquisition::http_client::HeadResponse>,
    Option<PatternResult>,
    Vec<HttpAction>,
);

/// A page rendered via browser (Layer 3 fallback).
struct BrowserRenderedPage {
    url: String,
    final_url: String,
    extraction: crate::extraction::loader::ExtractionResult,
    nav_result: crate::renderer::NavigationResult,
    discovered_links: Vec<String>,
}

/// Select diverse sample URLs for HTTP GET.
///
/// Prioritizes: homepage, URLs from different path prefixes, different page types.
fn select_diverse_samples(
    html_urls: &[String],
    _all_urls: &[String],
    domain: &str,
    count: usize,
) -> Vec<String> {
    if html_urls.len() <= count {
        return html_urls.to_vec();
    }

    let mut selected = Vec::new();
    let mut seen_prefixes: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Always include homepage
    let entry = format!("https://{domain}");
    let entry_slash = format!("https://{domain}/");
    for url in html_urls {
        if url == &entry || url == &entry_slash {
            selected.push(url.clone());
            break;
        }
    }

    // Classify all URLs and group by type
    let mut by_type: HashMap<PageType, Vec<&String>> = HashMap::new();
    for url in html_urls {
        let (pt, _) = url_classifier::classify_url(url, domain);
        by_type.entry(pt).or_default().push(url);
    }

    // Round-robin across types to get diversity
    let types: Vec<PageType> = by_type.keys().copied().collect();
    let mut type_idx = 0;
    while selected.len() < count && selected.len() < html_urls.len() {
        let pt = &types[type_idx % types.len()];
        if let Some(urls) = by_type.get_mut(pt) {
            if let Some(url) = urls.pop() {
                let prefix = url_prefix(url);
                if (!seen_prefixes.contains(&prefix) || selected.len() < count / 2)
                    && !selected.contains(url)
                {
                    selected.push(url.clone());
                    seen_prefixes.insert(prefix);
                }
            }
        }
        type_idx += 1;

        // Safety: if we've gone through all types without adding, just fill from remaining
        if type_idx > types.len() * 3 {
            for url in html_urls {
                if selected.len() >= count {
                    break;
                }
                if !selected.contains(url) {
                    selected.push(url.clone());
                }
            }
            break;
        }
    }

    selected
}

fn url_prefix(url: &str) -> String {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    if let Some(slash_pos) = rest.find('/') {
        let path = &rest[slash_pos..];
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if let Some(first) = parts.first() {
            return format!("/{first}");
        }
    }
    "/".to_string()
}

/// Infer edges between pages based on URL path structure.
fn infer_edges_from_url_structure(
    classified: &[(String, PageType, f32)],
    url_to_index: &HashMap<String, u32>,
    _rendered_set: &HashMap<String, usize>,
    builder: &mut SiteMapBuilder,
) {
    let mut by_parent: HashMap<String, Vec<u32>> = HashMap::new();
    let mut edges_added = 0u32;

    for (url, _pt, _conf) in classified {
        if let Some(&idx) = url_to_index.get(url) {
            let parent = parent_path(url);
            by_parent.entry(parent).or_default().push(idx);

            // Connect every node bidirectionally to root
            if idx != 0 {
                builder.add_edge(0, idx, EdgeType::Navigation, 2, EdgeFlags::default());
                builder.add_edge(idx, 0, EdgeType::Navigation, 3, EdgeFlags::default());
                edges_added += 2;
            }
        }
    }

    for (parent_path_str, children) in &by_parent {
        for (url, _pt, _conf) in classified {
            if url.ends_with(parent_path_str) || url.ends_with(&format!("{parent_path_str}/")) {
                if let Some(&parent_idx) = url_to_index.get(url) {
                    for &child_idx in children {
                        if parent_idx != child_idx {
                            builder.add_edge(
                                parent_idx,
                                child_idx,
                                EdgeType::Navigation,
                                2,
                                EdgeFlags::default(),
                            );
                            builder.add_edge(
                                child_idx,
                                parent_idx,
                                EdgeType::Navigation,
                                3,
                                EdgeFlags::default(),
                            );
                            edges_added += 2;
                        }
                    }
                }
            }
        }

        if children.len() >= 2 && children.len() <= 100 {
            for i in 0..children.len().min(20) {
                let next = (i + 1) % children.len();
                if children[i] != children[next] {
                    builder.add_edge(
                        children[i],
                        children[next],
                        EdgeType::Navigation,
                        1,
                        EdgeFlags::default(),
                    );
                    edges_added += 1;
                }
            }
        }
    }

    if edges_added > 0 {
        tracing::info!("inferred {edges_added} edges from URL structure");
    }
}

fn parent_path(url: &str) -> String {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    if let Some(slash_pos) = rest.find('/') {
        let path = &rest[slash_pos..];
        if let Some(last_slash) = path.rfind('/') {
            if last_slash > 0 {
                return path[..last_slash].to_string();
            }
        }
    }
    "/".to_string()
}

/// Extract URLs from embedded JavaScript state and `<link>` tags.
///
/// Many SPAs embed data as JSON in `<script>` tags (`__NEXT_DATA__`,
/// `window.__INITIAL_STATE__`, etc.) or reference pages via `<link>` tags.
/// This function extracts internal URLs from both sources.
fn extract_urls_from_page_source(html: &str, domain: &str) -> Vec<String> {
    use scraper::{Html, Selector};

    let mut urls = Vec::new();
    let document = Html::parse_document(html);

    // 1. Extract URLs from <link> tags (alternate, preload, etc.)
    if let Ok(sel) = Selector::parse("link[href]") {
        for el in document.select(&sel) {
            if let Some(href) = el.value().attr("href") {
                let full_url = if href.starts_with("https://") || href.starts_with("http://") {
                    href.to_string()
                } else if href.starts_with('/') && !href.starts_with("//") {
                    format!("https://{domain}{href}")
                } else {
                    continue;
                };
                // Only keep HTML-like URLs that belong to this domain
                if (full_url.contains(domain) || full_url.contains(&format!("www.{domain}")))
                    && !full_url.ends_with(".css")
                    && !full_url.ends_with(".js")
                    && !full_url.ends_with(".png")
                    && !full_url.ends_with(".jpg")
                    && !full_url.ends_with(".ico")
                    && !full_url.ends_with(".woff2")
                    && !full_url.ends_with(".woff")
                    && !urls.contains(&full_url)
                {
                    urls.push(full_url);
                }
            }
        }
    }

    // 2. Extract URLs from <script> tag content (JSON state embeds)
    if let Ok(sel) = Selector::parse("script") {
        let domain_escaped = domain.replace('.', r"\.");
        let url_pattern = format!(
            r#"https?://(?:www\.)?{domain_escaped}(/[^"'\s<>\{{}}\\]{{1,500}})"#
        );
        let url_re = match regex::Regex::new(&url_pattern) {
            Ok(re) => re,
            Err(_) => return urls,
        };

        for el in document.select(&sel) {
            let text = el.inner_html();
            if text.len() < 100 {
                continue; // Skip tiny scripts
            }
            for mat in url_re.find_iter(&text) {
                let url = mat.as_str().to_string();
                // Skip asset URLs
                if url.ends_with(".js")
                    || url.ends_with(".css")
                    || url.ends_with(".png")
                    || url.ends_with(".jpg")
                    || url.ends_with(".svg")
                    || url.ends_with(".woff2")
                    || url.contains("/static/")
                    || url.contains("/_next/")
                    || url.contains("/assets/")
                {
                    continue;
                }
                if !urls.contains(&url) {
                    urls.push(url);
                }
            }
        }
    }

    // Cap at 200 URLs to avoid overwhelming the mapper
    urls.truncate(200);
    urls
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_urls_from_page_source() {
        let html = r#"
        <html><head>
        <link rel="alternate" href="https://example.com/blog" />
        <link rel="stylesheet" href="/style.css" />
        <script>
            window.__DATA__ = {"pages": ["https://example.com/products/1", "https://example.com/about"]};
        </script>
        </head><body></body></html>
        "#;
        let urls = extract_urls_from_page_source(html, "example.com");
        assert!(urls.iter().any(|u| u.contains("/blog")));
        assert!(urls.iter().any(|u| u.contains("/products/1")));
        assert!(urls.iter().any(|u| u.contains("/about")));
        // CSS file should be excluded
        assert!(!urls.iter().any(|u| u.ends_with(".css")));
    }

    #[test]
    fn test_parent_path() {
        assert_eq!(parent_path("https://example.com/blog/post-1"), "/blog");
        assert_eq!(parent_path("https://example.com/a/b/c"), "/a/b");
        assert_eq!(parent_path("https://example.com/page"), "/");
    }

    #[test]
    fn test_url_prefix() {
        assert_eq!(url_prefix("https://example.com/blog/post-1"), "/blog");
        assert_eq!(url_prefix("https://example.com/"), "/");
        assert_eq!(url_prefix("https://example.com/products/123"), "/products");
    }

    #[test]
    fn test_select_diverse_samples() {
        let urls = vec![
            "https://example.com/".to_string(),
            "https://example.com/blog/post-1".to_string(),
            "https://example.com/blog/post-2".to_string(),
            "https://example.com/products/widget".to_string(),
            "https://example.com/about".to_string(),
        ];
        let selected = select_diverse_samples(&urls, &urls, "example.com", 3);
        assert!(selected.len() <= 3);
        assert!(!selected.is_empty());
    }
}
