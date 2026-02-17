//! Mapper: orchestrates the entire mapping process.
//!
//! This is the core of the cartography engine. It:
//! 1. Fetches robots.txt and sitemap.xml
//! 2. Discovers URLs via sitemap or crawling
//! 3. Classifies URLs by pattern
//! 4. Selects samples for rendering
//! 5. Renders and extracts features from samples
//! 6. Interpolates features for unrendered pages
//! 7. Builds edges from discovered links
//! 8. Assembles the final SiteMap

use crate::cartography::{
    action_encoder, crawler::Crawler, feature_encoder, interpolator, page_classifier,
    rate_limiter::RateLimiter, robots, sampler, sitemap, url_classifier,
};
use crate::extraction::loader::ExtractionLoader;
use crate::map::builder::SiteMapBuilder;
use crate::map::types::*;
use crate::renderer::Renderer;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

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

    /// Map an entire site. Returns a complete SiteMap.
    pub async fn map(&self, request: MapRequest) -> Result<SiteMap> {
        let entry_url = format!("https://{}", request.domain);
        info!("mapping {} (max_nodes={}, max_render={})", request.domain, request.max_nodes, request.max_render);

        // 1. Fetch robots.txt
        let robots_rules = self.fetch_robots(&request.domain, request.respect_robots).await;
        let crawl_delay = robots_rules.as_ref().and_then(|r| r.crawl_delay);

        // 2. Create rate limiter from robots rules
        let rate_limiter = Arc::new(RateLimiter::from_crawl_delay(crawl_delay, 5));

        // 3. Fetch sitemap URLs
        let sitemap_entries = self.fetch_sitemap_urls(&request.domain, &robots_rules).await;

        // 4. Collect all URLs (from sitemap or by crawling)
        let all_urls = if sitemap_entries.is_empty() {
            info!("no sitemap found, crawling from entry point");
            let crawler = Crawler::new(
                Arc::clone(&self.renderer),
                Arc::clone(&self.extractor_loader),
                Arc::clone(&rate_limiter),
            );
            let pages = crawler
                .crawl_and_discover(std::slice::from_ref(&entry_url), request.max_render as usize)
                .await;

            // Build map directly from crawled pages
            return self.build_map_from_crawled(
                &request.domain,
                pages,
                request.max_nodes,
            );
        } else {
            info!("found {} URLs from sitemap", sitemap_entries.len());
            let mut urls: Vec<String> = sitemap_entries.iter().map(|e| e.url.clone()).collect();
            // Ensure entry URL is included
            if !urls.contains(&entry_url) {
                urls.insert(0, entry_url);
            }
            // Limit to max_nodes
            urls.truncate(request.max_nodes as usize);
            urls
        };

        // 5. Classify all URLs
        let classified: Vec<(String, PageType, f32)> = all_urls
            .iter()
            .map(|url| {
                let (pt, conf) = url_classifier::classify_url(url, &request.domain);
                (url.clone(), pt, conf)
            })
            .collect();

        // 6. Select samples for rendering
        let sample_urls = sampler::select_samples(&classified, request.max_render as usize);
        info!("selected {} pages for rendering", sample_urls.len());

        // 7. Render samples
        let crawler = Crawler::new(
            Arc::clone(&self.renderer),
            Arc::clone(&self.extractor_loader),
            rate_limiter,
        );

        let mut rendered_pages = Vec::new();
        for url in &sample_urls {
            match crawler.render_page(url).await {
                Ok(page) => rendered_pages.push(page),
                Err(e) => {
                    tracing::warn!("failed to render {url}: {e}");
                }
            }
        }

        // 8. Build map from classified URLs + rendered pages
        self.build_map_from_classified(
            &request.domain,
            &classified,
            &rendered_pages,
        )
    }

    async fn fetch_robots(
        &self,
        domain: &str,
        respect_robots: bool,
    ) -> Option<robots::RobotsRules> {
        if !respect_robots {
            return None;
        }

        let url = format!("https://{domain}/robots.txt");
        let resp = reqwest::get(&url).await.ok()?;
        let text = resp.text().await.ok()?;
        Some(robots::parse_robots(&text, "cortex"))
    }

    async fn fetch_sitemap_urls(
        &self,
        domain: &str,
        robots_rules: &Option<robots::RobotsRules>,
    ) -> Vec<sitemap::SitemapEntry> {
        let mut sitemap_urls = Vec::new();

        // Get sitemap URLs from robots.txt
        if let Some(rules) = robots_rules {
            sitemap_urls.extend(rules.sitemaps.clone());
        }

        // Default sitemap location
        let default_sitemap = format!("https://{domain}/sitemap.xml");
        if !sitemap_urls.contains(&default_sitemap) {
            sitemap_urls.push(default_sitemap);
        }

        let mut all_entries = Vec::new();
        for url in &sitemap_urls {
            if let Ok(resp) = reqwest::get(url).await {
                if let Ok(xml) = resp.text().await {
                    if let Ok(entries) = sitemap::parse_sitemap(&xml) {
                        all_entries.extend(entries);
                    }
                }
            }
        }

        all_entries
    }

    fn build_map_from_crawled(
        &self,
        domain: &str,
        pages: Vec<crate::cartography::crawler::DiscoveredPage>,
        max_nodes: u32,
    ) -> Result<SiteMap> {
        let mut builder = SiteMapBuilder::new(domain);
        let mut url_to_index: HashMap<String, u32> = HashMap::new();

        // Add nodes for each crawled page
        for page in &pages {
            if url_to_index.len() as u32 >= max_nodes {
                break;
            }

            let (page_type, confidence) = page_classifier::classify_page(
                &page.extraction,
                &page.url,
            );

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

            // Set computed flags (HAS_PRICE, HAS_MEDIA, HAS_FORM, etc.)
            builder.merge_flags(idx, encode_result.flags);

            // Add actions
            let actions = action_encoder::encode_actions_from_json(&page.extraction.actions);
            for action in actions {
                builder.add_action(idx, action.opcode, action.target_node, action.cost_hint, action.risk);
            }

            // Mark as rendered
            builder.set_rendered(idx, encode_result.features);
        }

        // Add edges from discovered links
        for page in &pages {
            let from_idx = match url_to_index.get(&page.url) {
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
                    }
                }
            }
        }

        Ok(builder.build())
    }

    fn build_map_from_classified(
        &self,
        domain: &str,
        classified: &[(String, PageType, f32)],
        rendered_pages: &[crate::cartography::crawler::DiscoveredPage],
    ) -> Result<SiteMap> {
        let mut builder = SiteMapBuilder::new(domain);
        let mut url_to_index: HashMap<String, u32> = HashMap::new();

        // Build a set of rendered URLs for quick lookup
        let rendered_set: HashMap<String, usize> = rendered_pages
            .iter()
            .enumerate()
            .map(|(i, p)| (p.url.clone(), i))
            .collect();

        // Collect rendered features by PageType for interpolation
        let mut rendered_features_by_type: HashMap<PageType, Vec<[f32; FEATURE_DIM]>> =
            HashMap::new();

        // First pass: add rendered nodes
        for page in rendered_pages {
            let (page_type, confidence) =
                page_classifier::classify_page(&page.extraction, &page.url);

            let encode_result = feature_encoder::encode_features_with_flags(
                &page.extraction,
                &page.nav_result,
                &page.url,
                page_type,
                confidence,
            );

            rendered_features_by_type
                .entry(page_type)
                .or_default()
                .push(encode_result.features);

            let idx = builder.add_node(
                &page.url,
                page_type,
                encode_result.features,
                (confidence * 255.0) as u8,
            );
            url_to_index.insert(page.url.clone(), idx);

            // Set computed flags (HAS_PRICE, HAS_MEDIA, HAS_FORM, etc.)
            builder.merge_flags(idx, encode_result.flags);

            // Add actions
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

            builder.set_rendered(idx, encode_result.features);
        }

        // Second pass: add unrendered nodes with interpolated features
        for (url, page_type, confidence) in classified {
            if url_to_index.contains_key(url) {
                continue; // Already added as rendered
            }

            // Interpolate features from rendered samples of same type
            let samples: Vec<&[f32; FEATURE_DIM]> = rendered_features_by_type
                .get(page_type)
                .map(|v| v.iter().collect())
                .unwrap_or_default();

            let features = interpolator::interpolate_features(*page_type, &samples);

            let idx = builder.add_node(url, *page_type, features, (*confidence * 255.0) as u8);
            url_to_index.insert(url.clone(), idx);
        }

        // Add edges from rendered pages' links
        for page in rendered_pages {
            let from_idx = match url_to_index.get(&page.url) {
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
                    }
                }
            }
        }

        // Infer edges for unrendered pages from URL structure
        infer_edges_from_url_structure(classified, &url_to_index, &rendered_set, &mut builder);

        Ok(builder.build())
    }
}

/// Infer edges between unrendered pages based on URL path structure.
/// Pages with the same path prefix are likely linked.
fn infer_edges_from_url_structure(
    classified: &[(String, PageType, f32)],
    url_to_index: &HashMap<String, u32>,
    rendered_set: &HashMap<String, usize>,
    builder: &mut SiteMapBuilder,
) {
    // Group URLs by their parent path
    let mut by_parent: HashMap<String, Vec<u32>> = HashMap::new();

    for (url, _pt, _conf) in classified {
        if rendered_set.contains_key(url) {
            continue; // Skip rendered pages (they have real edges)
        }
        if let Some(&idx) = url_to_index.get(url) {
            let parent = parent_path(url);
            by_parent.entry(parent).or_default().push(idx);
        }
    }

    // Add edges between siblings (same parent path)
    // and from parent pages to children
    for (parent_path_str, children) in &by_parent {
        // Find parent node
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
                                EdgeFlags(EdgeFlags::REQUIRES_FORM), // Estimated edge
                            );
                        }
                    }
                }
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parent_path() {
        assert_eq!(
            parent_path("https://example.com/blog/post-1"),
            "/blog"
        );
        assert_eq!(
            parent_path("https://example.com/a/b/c"),
            "/a/b"
        );
        assert_eq!(parent_path("https://example.com/page"), "/");
    }
}
