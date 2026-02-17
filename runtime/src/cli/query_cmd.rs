//! `cortex query <domain>` — query a mapped site for matching pages.

use crate::cli::output;
use crate::intelligence::cache::MapCache;
use crate::map::types::{FeatureRange, NodeQuery, PageType, FEAT_PRICE, FEAT_RATING};
use anyhow::{bail, Result};

/// Run the query command.
pub async fn run(
    domain: &str,
    page_type: Option<&str>,
    price_lt: Option<f32>,
    rating_gt: Option<f32>,
    limit: u32,
    feature_filters: &[String],
) -> Result<()> {

    // Load cached map
    let mut cache = MapCache::default_cache()?;
    let map = match cache.load_map(domain)? {
        Some(m) => m,
        None => {
            if output::is_json() {
                output::print_json(&serde_json::json!({
                    "error": "no_map",
                    "message": format!("No cached map for '{domain}'"),
                    "hint": format!("Run: cortex map {domain}")
                }));
                return Ok(());
            }
            bail!(
                "No map found for '{domain}'. Run 'cortex map {domain}' first."
            );
        }
    };

    // Build query
    let mut feature_ranges = Vec::new();

    let page_types = page_type.map(|t| {
        let pt = parse_page_type(t);
        if matches!(pt, PageType::Unknown) && !output::is_quiet() {
            eprintln!(
                "  Warning: unknown page type '{t}'. Did you mean one of: home, product_detail, article, search_results, login?"
            );
        }
        vec![pt]
    });

    if let Some(price) = price_lt {
        feature_ranges.push(FeatureRange {
            dimension: FEAT_PRICE,
            min: None,
            max: Some(price),
        });
    }

    if let Some(rating) = rating_gt {
        feature_ranges.push(FeatureRange {
            dimension: FEAT_RATING,
            min: Some(rating),
            max: None,
        });
    }

    // Parse --feature "48<300" style filters
    for f in feature_filters {
        if let Some(range) = parse_feature_filter(f) {
            feature_ranges.push(range);
        } else if !output::is_quiet() {
            eprintln!("  Warning: could not parse feature filter '{f}'. Use format: \"48<300\" or \"52>0.8\"");
        }
    }

    let query = NodeQuery {
        page_types,
        feature_ranges,
        limit: limit as usize,
        ..Default::default()
    };

    let results = map.filter(&query);

    if output::is_json() {
        let items: Vec<serde_json::Value> = results
            .iter()
            .map(|m| {
                serde_json::json!({
                    "index": m.index,
                    "url": m.url,
                    "page_type": format!("{:?}", m.page_type),
                    "confidence": m.confidence,
                })
            })
            .collect();
        output::print_json(&serde_json::json!({
            "domain": domain,
            "total": results.len(),
            "results": items,
        }));
        return Ok(());
    }

    if results.is_empty() {
        if !output::is_quiet() {
            eprintln!("  No matching pages found. Try broader filters.");
        }
        return Ok(());
    }

    if !output::is_quiet() {
        let total = results.len();
        if total > limit as usize {
            eprintln!(
                "  Found {} matching pages. Showing first {} (use --limit to change).",
                total, limit
            );
        } else {
            eprintln!("  Found {} matching page(s):", total);
        }
        eprintln!();

        for m in &results {
            let truncated_url = if m.url.len() > 50 {
                format!("{}...", &m.url[..47])
            } else {
                m.url.clone()
            };
            eprintln!(
                "    [{:>5}] {:<20} {:<50} conf: {:.2}",
                m.index,
                format!("{:?}", m.page_type),
                truncated_url,
                m.confidence,
            );
        }
    }

    Ok(())
}

/// Parse a page type string to the enum, supporting both human and hex formats.
fn parse_page_type(s: &str) -> PageType {
    // Try hex format first (e.g., "0x04")
    if let Some(hex_str) = s.strip_prefix("0x") {
        if let Ok(n) = u8::from_str_radix(hex_str, 16) {
            return PageType::from_u8(n);
        }
    }

    // Try decimal
    if let Ok(n) = s.parse::<u8>() {
        return PageType::from_u8(n);
    }

    // Named types
    match s.to_lowercase().as_str() {
        "home" => PageType::Home,
        "product" | "product_detail" => PageType::ProductDetail,
        "product_listing" | "listing" | "category" => PageType::ProductListing,
        "article" | "blog" | "post" => PageType::Article,
        "search" | "search_results" => PageType::SearchResults,
        "login" | "signin" => PageType::Login,
        "cart" | "basket" => PageType::Cart,
        "checkout" => PageType::Checkout,
        "account" | "profile" => PageType::Account,
        "docs" | "documentation" => PageType::Documentation,
        "form" | "form_page" => PageType::FormPage,
        "about" | "about_page" => PageType::AboutPage,
        "contact" | "contact_page" => PageType::ContactPage,
        "faq" => PageType::Faq,
        "pricing" | "pricing_page" => PageType::PricingPage,
        _ => PageType::Unknown,
    }
}

/// Parse a feature filter like "48<300" or "52>0.8".
fn parse_feature_filter(s: &str) -> Option<FeatureRange> {
    // Try "<" separator
    if let Some(pos) = s.find('<') {
        let dim: usize = s[..pos].trim().parse().ok()?;
        let val: f32 = s[pos + 1..].trim().parse().ok()?;
        if dim > 127 {
            return None;
        }
        return Some(FeatureRange {
            dimension: dim,
            min: None,
            max: Some(val),
        });
    }

    // Try ">" separator
    if let Some(pos) = s.find('>') {
        let dim: usize = s[..pos].trim().parse().ok()?;
        let val: f32 = s[pos + 1..].trim().parse().ok()?;
        if dim > 127 {
            return None;
        }
        return Some(FeatureRange {
            dimension: dim,
            min: Some(val),
            max: None,
        });
    }

    None
}
