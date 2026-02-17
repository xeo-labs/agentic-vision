//! `cortex query <domain>` — query a mapped site for matching pages.

use crate::intelligence::cache::MapCache;
use crate::map::types::{FeatureRange, NodeQuery, PageType, FEAT_PRICE, FEAT_RATING};
use anyhow::{bail, Context, Result};

/// Run the query command.
pub async fn run(
    domain: &str,
    page_type: Option<&str>,
    price_lt: Option<f32>,
    rating_gt: Option<f32>,
    limit: u32,
) -> Result<()> {
    // Load cached map
    let cache = MapCache::default_cache()?;
    let map = cache
        .load_map(domain)?
        .ok_or_else(|| anyhow::anyhow!("No cached map for {domain}. Run: cortex map {domain}"))?;

    // Build query
    let mut feature_ranges = Vec::new();

    let page_types = page_type.map(|t| vec![parse_page_type(t)]);

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

    let query = NodeQuery {
        page_types,
        feature_ranges,
        limit: limit as usize,
        ..Default::default()
    };

    let results = map.filter(&query);

    if results.is_empty() {
        println!("No matching nodes found.");
        return Ok(());
    }

    println!("Found {} matching node(s):\n", results.len());
    for m in &results {
        println!(
            "  [{:>4}] {:20} {:<40} (confidence: {:.2})",
            m.index,
            format!("{:?}", m.page_type),
            m.url,
            m.confidence
        );
    }

    Ok(())
}

fn parse_page_type(s: &str) -> PageType {
    match s.to_lowercase().as_str() {
        "home" => PageType::Home,
        "product" | "product_detail" => PageType::ProductDetail,
        "product_listing" | "listing" => PageType::ProductListing,
        "article" | "blog" => PageType::Article,
        "search" | "search_results" => PageType::SearchResults,
        "login" => PageType::Login,
        "cart" => PageType::Cart,
        "checkout" => PageType::Checkout,
        "account" => PageType::Account,
        "docs" | "documentation" => PageType::Documentation,
        "form" => PageType::FormPage,
        "about" => PageType::AboutPage,
        "contact" => PageType::ContactPage,
        "faq" => PageType::Faq,
        "pricing" => PageType::PricingPage,
        _ => PageType::Unknown,
    }
}
