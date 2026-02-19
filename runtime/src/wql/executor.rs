//! WQL executor — runs query plans against compiled schemas and temporal store.

use crate::compiler::models::CompiledSchema;
use crate::compiler::schema;
use crate::map::types::*;
use crate::wql::planner::{PlanStep, QueryPlan};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A row returned from a WQL query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Row {
    /// Source domain.
    pub domain: String,
    /// Source URL.
    pub url: String,
    /// Source node ID.
    pub node_id: u32,
    /// Field values.
    pub fields: HashMap<String, Value>,
}

/// A value in a WQL result row.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Float(f64),
    Integer(i64),
    String(String),
    Bool(bool),
    Null,
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Float(v) => write!(f, "{v:.2}"),
            Value::Integer(v) => write!(f, "{v}"),
            Value::String(v) => write!(f, "{v}"),
            Value::Bool(v) => write!(f, "{v}"),
            Value::Null => write!(f, "null"),
        }
    }
}

/// Execute a WQL query plan against a set of domain maps.
///
/// `maps` is a map of domain → SiteMap for all available mapped sites.
pub fn execute(plan: &QueryPlan, maps: &HashMap<String, SiteMap>) -> Result<Vec<Row>> {
    let mut rows: Vec<Row> = Vec::new();

    // Find the scan step to get model and domains
    let (target_model, target_domains) = find_scan_target(plan)?;

    // Scan matching nodes from each domain
    for (domain, site_map) in maps {
        if !target_domains.is_empty() && !target_domains.contains(domain) {
            continue;
        }

        // Find nodes matching the model type
        let page_type = model_to_page_type(&target_model);
        for (idx, node) in site_map.nodes.iter().enumerate() {
            if node.page_type != page_type {
                continue;
            }

            let url = site_map.urls.get(idx).cloned().unwrap_or_default();
            let features = site_map.features.get(idx);

            let mut fields: HashMap<String, Value> = HashMap::new();

            // Standard fields
            fields.insert("url".to_string(), Value::String(url.clone()));
            fields.insert("node_id".to_string(), Value::Integer(idx as i64));

            // Map feature dimensions to field names
            if let Some(feats) = features {
                map_features_to_fields(feats, &target_model, &mut fields);
            }

            rows.push(Row {
                domain: domain.clone(),
                url,
                node_id: idx as u32,
                fields,
            });
        }
    }

    // Apply remaining plan steps
    for step in &plan.steps {
        match step {
            PlanStep::ScanModel { .. } => {} // Already done
            PlanStep::Filter { field, op, value } => {
                rows = filter_rows(rows, field, op, value);
            }
            PlanStep::Sort { field, ascending } => {
                sort_rows(&mut rows, field, *ascending);
            }
            PlanStep::Limit { n } => {
                rows.truncate(*n);
            }
            PlanStep::Project {
                fields: proj_fields,
            } => {
                for row in &mut rows {
                    row.fields.retain(|k, _| proj_fields.contains(k));
                }
            }
            _ => {} // Join, TemporalEnrich handled separately
        }
    }

    Ok(rows)
}

/// Find the scan target from the plan.
fn find_scan_target(plan: &QueryPlan) -> Result<(String, Vec<String>)> {
    for step in &plan.steps {
        if let PlanStep::ScanModel { model, domains } = step {
            return Ok((model.clone(), domains.clone()));
        }
    }
    anyhow::bail!("no ScanModel step in plan")
}

/// Map PageType for a model name.
fn model_to_page_type(model: &str) -> PageType {
    match model {
        "Product" => PageType::ProductDetail,
        "Category" | "ProductListing" => PageType::ProductListing,
        "Article" => PageType::Article,
        "Review" => PageType::ReviewList,
        "FAQ" => PageType::Faq,
        "Organization" => PageType::AboutPage,
        "Contact" => PageType::ContactPage,
        "Cart" => PageType::Cart,
        "Checkout" => PageType::Checkout,
        "Account" => PageType::Account,
        "Media" => PageType::MediaPage,
        "Site" | "WebSite" | "Home" => PageType::Home,
        "Event" => PageType::Calendar,
        "Documentation" | "Docs" => PageType::Documentation,
        "Forum" | "Discussion" => PageType::Forum,
        "Search" | "SearchResults" => PageType::SearchResults,
        _ => PageType::Unknown,
    }
}

/// Map feature vector dimensions to named fields.
fn map_features_to_fields(
    feats: &[f32; FEATURE_DIM],
    model: &str,
    fields: &mut HashMap<String, Value>,
) {
    let dim_map: &[(&str, usize)] = match model {
        "Product" => &[
            ("price", FEAT_PRICE),
            ("original_price", FEAT_PRICE_ORIGINAL),
            ("discount_percent", FEAT_DISCOUNT_PCT),
            ("availability", FEAT_AVAILABILITY),
            ("rating", FEAT_RATING),
            ("review_count", FEAT_REVIEW_COUNT_LOG),
            ("seller_reputation", FEAT_SELLER_REPUTATION),
            ("variant_count", FEAT_VARIANT_COUNT),
            ("deal_score", FEAT_DEAL_SCORE),
        ],
        "Article" => &[
            ("word_count", FEAT_TEXT_LENGTH_LOG),
            ("reading_level", FEAT_READING_LEVEL),
            ("sentiment", FEAT_SENTIMENT),
            ("rating", FEAT_RATING),
            ("image_count", FEAT_IMAGE_COUNT),
        ],
        _ => &[("rating", FEAT_RATING), ("image_count", FEAT_IMAGE_COUNT)],
    };

    for (name, dim) in dim_map {
        let val = feats[*dim];
        if val != 0.0 {
            if *name == "review_count"
                || *name == "variant_count"
                || *name == "word_count"
                || *name == "image_count"
            {
                fields.insert(name.to_string(), Value::Integer(val as i64));
            } else {
                fields.insert(name.to_string(), Value::Float(val as f64));
            }
        }
    }
}

/// Filter rows by a comparison.
fn filter_rows(rows: Vec<Row>, field: &str, op: &str, value: &str) -> Vec<Row> {
    let threshold: Option<f64> = value.parse().ok();

    rows.into_iter()
        .filter(|row| {
            let field_val = row.fields.get(field);
            match (field_val, threshold) {
                (Some(Value::Float(v)), Some(t)) => match op {
                    "<" => *v < t,
                    ">" => *v > t,
                    "<=" => *v <= t,
                    ">=" => *v >= t,
                    "=" => (*v - t).abs() < 0.001,
                    "!=" => (*v - t).abs() >= 0.001,
                    _ => true,
                },
                (Some(Value::Integer(v)), Some(t)) => {
                    let v = *v as f64;
                    match op {
                        "<" => v < t,
                        ">" => v > t,
                        "<=" => v <= t,
                        ">=" => v >= t,
                        "=" => (v - t).abs() < 0.001,
                        "!=" => (v - t).abs() >= 0.001,
                        _ => true,
                    }
                }
                (Some(Value::String(v)), _) => match op {
                    "=" => v == value,
                    "!=" => v != value,
                    _ => true,
                },
                _ => false, // Missing field → filtered out
            }
        })
        .collect()
}

/// Sort rows by a field.
fn sort_rows(rows: &mut [Row], field: &str, ascending: bool) {
    rows.sort_by(|a, b| {
        let va = a.fields.get(field);
        let vb = b.fields.get(field);

        let cmp = match (va, vb) {
            (Some(Value::Float(a)), Some(Value::Float(b))) => {
                a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
            }
            (Some(Value::Integer(a)), Some(Value::Integer(b))) => a.cmp(b),
            (Some(Value::String(a)), Some(Value::String(b))) => a.cmp(b),
            _ => std::cmp::Ordering::Equal,
        };

        if ascending {
            cmp
        } else {
            cmp.reverse()
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::builder::SiteMapBuilder;
    use crate::wql::{parser, planner};

    fn build_test_maps() -> HashMap<String, SiteMap> {
        let mut builder = SiteMapBuilder::new("shop.com");

        for i in 0..10 {
            let mut feats = [0.0f32; FEATURE_DIM];
            feats[FEAT_PRICE] = 50.0 + (i as f32 * 20.0);
            feats[FEAT_RATING] = 3.0 + (i as f32 * 0.2);
            builder.add_node(
                &format!("https://shop.com/product/{i}"),
                PageType::ProductDetail,
                feats,
                200,
            );
        }

        let map = builder.build();
        let mut maps = HashMap::new();
        maps.insert("shop.com".to_string(), map);
        maps
    }

    #[test]
    fn test_execute_simple_query() {
        let maps = build_test_maps();
        let query = parser::parse("SELECT * FROM Product LIMIT 5").unwrap();
        let plan_result = planner::plan(&query, None).unwrap();

        let rows = execute(&plan_result, &maps).unwrap();
        assert_eq!(rows.len(), 5);
    }

    #[test]
    fn test_execute_with_filter() {
        let maps = build_test_maps();
        let query = parser::parse("SELECT * FROM Product WHERE price < 100 LIMIT 20").unwrap();
        let plan_result = planner::plan(&query, None).unwrap();

        let rows = execute(&plan_result, &maps).unwrap();
        // Products with price < 100: 50, 70, 90 → 3 products
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn test_execute_with_order() {
        let maps = build_test_maps();
        let query = parser::parse("SELECT * FROM Product ORDER BY price DESC LIMIT 3").unwrap();
        let plan_result = planner::plan(&query, None).unwrap();

        let rows = execute(&plan_result, &maps).unwrap();
        assert_eq!(rows.len(), 3);

        // Check descending order
        if let (Some(Value::Float(a)), Some(Value::Float(b))) =
            (rows[0].fields.get("price"), rows[1].fields.get("price"))
        {
            assert!(a >= b, "should be descending");
        }
    }

    #[test]
    fn test_execute_empty_maps() {
        let maps = HashMap::new();
        let query = parser::parse("SELECT * FROM Product LIMIT 10").unwrap();
        let plan_result = planner::plan(&query, None).unwrap();

        let rows = execute(&plan_result, &maps).unwrap();
        assert!(rows.is_empty());
    }

    // ── v4 Test Suite: Phase 4 — WQL ──

    fn build_multi_domain_maps() -> HashMap<String, SiteMap> {
        let mut maps = HashMap::new();

        // Amazon-like: 15 products
        let mut b1 = SiteMapBuilder::new("amazon.com");
        for i in 0..15 {
            let mut feats = [0.0f32; FEATURE_DIM];
            feats[FEAT_PRICE] = 30.0 + (i as f32 * 25.0);
            feats[FEAT_RATING] = 3.0 + (i as f32 * 0.1);
            b1.add_node(
                &format!("https://amazon.com/product/{i}"),
                PageType::ProductDetail,
                feats,
                200,
            );
        }
        maps.insert("amazon.com".to_string(), b1.build());

        // BestBuy-like: 10 products
        let mut b2 = SiteMapBuilder::new("bestbuy.com");
        for i in 0..10 {
            let mut feats = [0.0f32; FEATURE_DIM];
            feats[FEAT_PRICE] = 100.0 + (i as f32 * 50.0);
            feats[FEAT_RATING] = 3.5 + (i as f32 * 0.15);
            b2.add_node(
                &format!("https://bestbuy.com/product/{i}"),
                PageType::ProductDetail,
                feats,
                200,
            );
        }
        maps.insert("bestbuy.com".to_string(), b2.build());

        // News site: 8 articles
        let mut b3 = SiteMapBuilder::new("bbc.com");
        for i in 0..8 {
            let mut feats = [0.0f32; FEATURE_DIM];
            feats[FEAT_TEXT_LENGTH_LOG] = 3.0 + (i as f32 * 0.2);
            feats[FEAT_READING_LEVEL] = 0.5;
            b3.add_node(
                &format!("https://bbc.com/article/{i}"),
                PageType::Article,
                feats,
                200,
            );
        }
        maps.insert("bbc.com".to_string(), b3.build());

        maps
    }

    #[test]
    fn test_v4_wql_simple_select() {
        let maps = build_multi_domain_maps();
        let query = parser::parse("SELECT * FROM Product LIMIT 5").unwrap();
        let plan = planner::plan(&query, None).unwrap();
        let rows = execute(&plan, &maps).unwrap();

        assert_eq!(rows.len(), 5);
        for row in &rows {
            assert!(
                row.fields.contains_key("price"),
                "Product rows should have price"
            );
        }
    }

    #[test]
    fn test_v4_wql_filtered_select() {
        let maps = build_multi_domain_maps();
        let query = parser::parse("SELECT * FROM Product WHERE price < 200 LIMIT 50").unwrap();
        let plan = planner::plan(&query, None).unwrap();
        let rows = execute(&plan, &maps).unwrap();

        for row in &rows {
            if let Some(Value::Float(p)) = row.fields.get("price") {
                assert!(*p < 200.0, "price {} should be < 200", p);
            }
        }
    }

    #[test]
    fn test_v4_wql_order_by_asc() {
        let maps = build_test_maps();
        let query =
            parser::parse("SELECT * FROM Product WHERE price < 500 ORDER BY price ASC LIMIT 10")
                .unwrap();
        let plan = planner::plan(&query, None).unwrap();
        let rows = execute(&plan, &maps).unwrap();

        if rows.len() >= 2 {
            let prices: Vec<f64> = rows
                .iter()
                .filter_map(|r| match r.fields.get("price") {
                    Some(Value::Float(p)) => Some(*p),
                    _ => None,
                })
                .collect();

            for w in prices.windows(2) {
                assert!(
                    w[0] <= w[1],
                    "prices should be ascending: {} <= {}",
                    w[0],
                    w[1]
                );
            }
        }
    }

    #[test]
    fn test_v4_wql_multi_domain_query() {
        let maps = build_multi_domain_maps();
        let query =
            parser::parse("SELECT * FROM Product ACROSS amazon.com, bestbuy.com LIMIT 30").unwrap();
        let plan = planner::plan(&query, None).unwrap();
        let rows = execute(&plan, &maps).unwrap();

        assert!(rows.len() > 0, "should find products across domains");

        let domains: std::collections::HashSet<&str> =
            rows.iter().map(|r| r.domain.as_str()).collect();
        assert!(
            domains.len() >= 2,
            "should have results from multiple domains, got {:?}",
            domains
        );
    }

    #[test]
    fn test_v4_wql_article_query() {
        let maps = build_multi_domain_maps();
        let query = parser::parse("SELECT * FROM Article LIMIT 5").unwrap();
        let plan = planner::plan(&query, None).unwrap();
        let rows = execute(&plan, &maps).unwrap();

        assert_eq!(rows.len(), 5);
        for row in &rows {
            assert_eq!(row.domain, "bbc.com", "articles should come from bbc.com");
        }
    }

    #[test]
    fn test_v4_wql_combined_filter_and_order() {
        let maps = build_multi_domain_maps();
        let query = parser::parse(
            "SELECT * FROM Product WHERE price < 300 AND rating > 3.0 ORDER BY price ASC LIMIT 10",
        )
        .unwrap();
        let plan = planner::plan(&query, None).unwrap();
        let rows = execute(&plan, &maps).unwrap();

        for row in &rows {
            if let Some(Value::Float(p)) = row.fields.get("price") {
                assert!(*p < 300.0);
            }
            if let Some(Value::Float(r)) = row.fields.get("rating") {
                assert!(*r > 3.0);
            }
        }
    }

    #[test]
    fn test_v4_wql_no_results() {
        let maps = build_test_maps();
        let query = parser::parse("SELECT * FROM Product WHERE price > 99999 LIMIT 10").unwrap();
        let plan = planner::plan(&query, None).unwrap();
        let rows = execute(&plan, &maps).unwrap();
        assert!(rows.is_empty(), "impossible filter should yield no results");
    }

    #[test]
    fn test_v4_wql_limit_respected() {
        let maps = build_multi_domain_maps();
        let query = parser::parse("SELECT * FROM Product LIMIT 3").unwrap();
        let plan = planner::plan(&query, None).unwrap();
        let rows = execute(&plan, &maps).unwrap();
        assert_eq!(rows.len(), 3, "limit should be respected");
    }
}
