//! v1.0 Scoring Integration Test
//!
//! Validates the v1.0 scoring rubric across all new platform capabilities:
//! - Compilability (schema inference, model discovery, client generation)
//! - Temporal readiness (registry push, history queryable)
//! - WQL queryability (site data queryable via WQL)
//!
//! Uses synthetic multi-domain data to simulate the 100-site re-test scoring.

use chrono::Utc;
use cortex_runtime::collective::delta::{compute_delta, strip_private_data};
use cortex_runtime::collective::registry::LocalRegistry;
use cortex_runtime::compiler::codegen;
use cortex_runtime::compiler::schema::infer_schema;
use cortex_runtime::map::builder::SiteMapBuilder;
use cortex_runtime::map::types::*;
use cortex_runtime::temporal::patterns::{detect_patterns, predict, Pattern, TrendDirection};
use cortex_runtime::temporal::store::TemporalStore;
use cortex_runtime::temporal::watch::{NotifyTarget, WatchCondition, WatchManager, WatchRule};
use cortex_runtime::wql::{executor, parser, planner};
use std::collections::HashMap;
use std::sync::Arc;
use tempfile::TempDir;

// ── Test Domain Builders ──

fn build_ecommerce_map(domain: &str, products: usize) -> SiteMap {
    let mut builder = SiteMapBuilder::new(domain);

    // Homepage
    let mut home_feats = [0.0f32; FEATURE_DIM];
    home_feats[FEAT_HAS_STRUCTURED_DATA] = 1.0;
    home_feats[FEAT_LINK_COUNT_INTERNAL] = products as f32;
    builder.add_node(
        &format!("https://{domain}/"),
        PageType::Home,
        home_feats,
        200,
    );

    // Products
    for i in 0..products {
        let mut feats = [0.0f32; FEATURE_DIM];
        feats[FEAT_PRICE] = 20.0 + (i as f32 * 15.0);
        feats[FEAT_RATING] = 3.0 + (i as f32 % 20.0) / 10.0;
        feats[FEAT_AVAILABILITY] = if i % 5 == 0 { 0.0 } else { 1.0 };
        feats[FEAT_HAS_STRUCTURED_DATA] = 1.0;
        feats[FEAT_TEXT_LENGTH_LOG] = 3.5;

        builder.add_node(
            &format!("https://{domain}/product/{i}"),
            PageType::ProductDetail,
            feats,
            200,
        );

        // Add buy action
        let opcode = OpCode {
            category: 0x02,
            action: 0x00,
        }; // Cart: add_to_cart
        builder.add_action((i + 1) as u32, opcode, -1, 5, 10);
    }

    // Edges: home → each product
    for i in 0..products {
        builder.add_edge(0, (i + 1) as u32, EdgeType::Navigation, 1, EdgeFlags(0));
    }

    builder.build()
}

fn build_news_map(domain: &str, articles: usize) -> SiteMap {
    let mut builder = SiteMapBuilder::new(domain);

    // Homepage
    let mut home_feats = [0.0f32; FEATURE_DIM];
    home_feats[FEAT_HAS_STRUCTURED_DATA] = 1.0;
    home_feats[FEAT_LINK_COUNT_INTERNAL] = articles as f32;
    builder.add_node(
        &format!("https://{domain}/"),
        PageType::Home,
        home_feats,
        200,
    );

    // Articles
    for i in 0..articles {
        let mut feats = [0.0f32; FEATURE_DIM];
        feats[FEAT_TEXT_LENGTH_LOG] = 3.5 + (i as f32 * 0.1);
        feats[FEAT_HAS_STRUCTURED_DATA] = 1.0;
        feats[FEAT_HEADING_COUNT] = 5.0 + (i as f32 * 0.5);

        builder.add_node(
            &format!("https://{domain}/article/{i}"),
            PageType::Article,
            feats,
            200,
        );
    }

    // Edges
    for i in 0..articles {
        builder.add_edge(0, (i + 1) as u32, EdgeType::Navigation, 1, EdgeFlags(0));
    }

    builder.build()
}

fn build_docs_map(domain: &str, pages: usize) -> SiteMap {
    let mut builder = SiteMapBuilder::new(domain);

    // Docs root
    let mut root_feats = [0.0f32; FEATURE_DIM];
    root_feats[FEAT_HAS_STRUCTURED_DATA] = 1.0;
    builder.add_node(
        &format!("https://{domain}/docs"),
        PageType::Documentation,
        root_feats,
        200,
    );

    for i in 0..pages {
        let mut feats = [0.0f32; FEATURE_DIM];
        feats[FEAT_TEXT_LENGTH_LOG] = 4.0 + (i as f32 * 0.1);
        feats[FEAT_HAS_STRUCTURED_DATA] = 1.0;

        builder.add_node(
            &format!("https://{domain}/docs/page/{i}"),
            PageType::Documentation,
            feats,
            200,
        );
    }

    for i in 0..pages {
        builder.add_edge(0, (i + 1) as u32, EdgeType::Navigation, 1, EdgeFlags(0));
    }

    builder.build()
}

// ── V1.0 Scoring Tests ──

/// Test: compilability scoring — schema inference + model discovery + codegen
#[test]
fn test_v1_compilability_scoring() {
    let domains: Vec<(&str, usize)> = vec![
        ("shop1.com", 10),
        ("shop2.com", 5),
        ("shop3.com", 20),
        ("shop4.com", 15),
        ("shop5.com", 8),
    ];

    let mut total_score = 0;

    for (domain, product_count) in &domains {
        let map = build_ecommerce_map(domain, *product_count);
        let mut site_score = 0;

        // Can we compile this site? (8 points for models found)
        let schema = infer_schema(&map, domain);
        if !schema.models.is_empty() {
            site_score += 8;
        }

        // Do models have >= 3 fields? (4 points)
        if schema.models.iter().any(|m| m.fields.len() >= 3) {
            site_score += 4;
        }

        // Are actions discovered? (3 points)
        if !schema.actions.is_empty() {
            site_score += 3;
        }

        assert!(
            site_score > 0,
            "site {domain} should score > 0 for compilability"
        );
        total_score += site_score;
    }

    let avg = total_score as f32 / domains.len() as f32;
    assert!(
        avg >= 10.0,
        "average compilability score should be >= 10/15, got {avg}"
    );

    // Also verify codegen works for one site
    let map = build_ecommerce_map("codegen-test.com", 10);
    let schema = infer_schema(&map, "codegen-test.com");
    let generated = codegen::generate_all_in_memory(&schema);
    assert!(!generated.files.is_empty(), "codegen should produce files");
    let python_file = generated.files.iter().find(|f| f.filename == "client.py");
    assert!(
        python_file.is_some(),
        "should generate a Python client file"
    );
    assert!(
        !python_file.unwrap().content.is_empty(),
        "Python client should not be empty"
    );
}

/// Test: compilability for news sites
#[test]
fn test_v1_compilability_news() {
    let map = build_news_map("news-test.com", 20);
    let schema = infer_schema(&map, "news-test.com");

    assert!(
        !schema.models.is_empty(),
        "news site should produce at least one model"
    );

    // Articles should have content-related fields
    let article_model = schema.models.iter().find(|m| m.name.contains("Article"));
    assert!(
        article_model.is_some(),
        "news site should have an Article model"
    );
}

/// Test: temporal readiness scoring
#[test]
fn test_v1_temporal_readiness_scoring() {
    let dir = TempDir::new().unwrap();
    let mut registry = LocalRegistry::new(dir.path().to_path_buf()).unwrap();

    let domains: Vec<(&str, usize)> = vec![
        ("temporal-a.com", 5),
        ("temporal-b.com", 10),
        ("temporal-c.com", 3),
    ];

    let mut total_score = 0;

    for (domain, count) in &domains {
        let mut site_score = 0;

        // Push initial map
        let map1 = build_ecommerce_map(domain, *count);
        registry.push(domain, &map1, None).unwrap();

        // Is this site in the registry? (5 points)
        let entry = registry.pull(domain).unwrap();
        if entry.is_some() {
            site_score += 5;
        }

        // Push a second version with delta
        let map2 = build_ecommerce_map(domain, *count + 2);
        let delta = compute_delta(&map1, &map2, "test-instance");
        registry.push(domain, &map2, Some(delta)).unwrap();

        // Can we query history? (5 points)
        let since = Utc::now() - chrono::Duration::hours(1);
        let deltas = registry.pull_since(domain, since).unwrap();
        if deltas.is_some() {
            site_score += 5;
        }

        total_score += site_score;
    }

    let avg = total_score as f32 / domains.len() as f32;
    assert!(
        avg >= 8.0,
        "average temporal readiness should be >= 8/10, got {avg}"
    );
}

/// Test: temporal store history queries
#[test]
fn test_v1_temporal_store_queries() {
    let dir = TempDir::new().unwrap();
    let mut push_registry = LocalRegistry::new(dir.path().to_path_buf()).unwrap();

    // Push two versions of the same domain
    let map1 = build_ecommerce_map("hist-test.com", 5);
    push_registry.push("hist-test.com", &map1, None).unwrap();

    let map2 = build_ecommerce_map("hist-test.com", 7);
    let delta = compute_delta(&map1, &map2, "test");
    push_registry
        .push("hist-test.com", &map2, Some(delta))
        .unwrap();

    // Create read-only store
    let read_registry = Arc::new(LocalRegistry::new(dir.path().to_path_buf()).unwrap());
    let store = TemporalStore::new(read_registry);

    // Query history — should not error
    let since = Utc::now() - chrono::Duration::days(1);
    let history = store
        .history("hist-test.com", "/product/0", FEAT_PRICE as u8, since)
        .unwrap();
    assert!(history.len() <= 100, "history should not be unreasonable");

    // Diff query
    let diffs = store.diff("hist-test.com", "/product/0", since).unwrap();
    assert!(diffs.len() <= 100, "diffs should not be unreasonable");

    // History compare
    let pairs = vec![
        ("hist-test.com".to_string(), "/product/0".to_string()),
        ("hist-test.com".to_string(), "/product/1".to_string()),
    ];
    let compare = store
        .history_compare(&pairs, FEAT_PRICE as u8, since)
        .unwrap();
    assert_eq!(compare.len(), 2, "should have 2 comparison entries");
}

/// Test: WQL queryability scoring
#[test]
fn test_v1_wql_queryability_scoring() {
    let mut maps = HashMap::new();
    maps.insert(
        "wqlshop.com".to_string(),
        build_ecommerce_map("wqlshop.com", 10),
    );
    maps.insert("wqlnews.com".to_string(), build_news_map("wqlnews.com", 8));
    maps.insert("wqldocs.com".to_string(), build_docs_map("wqldocs.com", 5));

    let mut total_score = 0;

    // Use specific model types for each domain kind
    let domain_models: Vec<(&str, &str)> = vec![
        ("wqlshop.com", "Product"),
        ("wqlnews.com", "Article"),
        ("wqldocs.com", "Documentation"),
    ];

    for (domain, model) in &domain_models {
        let mut site_score = 0;

        // Can we WQL query this site?
        let query_str = format!("SELECT * FROM {model} ACROSS {domain} LIMIT 3");
        match parser::parse(&query_str) {
            Ok(query) => match planner::plan(&query, None) {
                Ok(plan) => match executor::execute(&plan, &maps) {
                    Ok(results) => {
                        if !results.is_empty() {
                            site_score += 10;
                        } else {
                            site_score += 5;
                        }
                    }
                    Err(e) => {
                        eprintln!("executor error for {domain}/{model}: {e}");
                        site_score += 2;
                    }
                },
                Err(e) => eprintln!("planner error for {domain}/{model}: {e}"),
            },
            Err(e) => eprintln!("parse error for {domain}/{model}: {e}"),
        }

        total_score += site_score;
    }

    let avg = total_score as f32 / domain_models.len() as f32;
    assert!(
        avg >= 5.0,
        "average WQL queryability should be >= 5/10, got {avg}"
    );
}

/// Test: WQL cross-domain query
#[test]
fn test_v1_wql_cross_domain() {
    let mut maps = HashMap::new();
    maps.insert("a.com".to_string(), build_ecommerce_map("a.com", 5));
    maps.insert("b.com".to_string(), build_ecommerce_map("b.com", 3));

    let query = parser::parse("SELECT * FROM Product ACROSS a.com, b.com LIMIT 20").unwrap();
    let plan = planner::plan(&query, None).unwrap();
    let results = executor::execute(&plan, &maps).unwrap();

    // Should find products across both domains
    assert!(
        !results.is_empty(),
        "cross-domain query should find results"
    );
}

/// Test: watch system integration with temporal data
#[test]
fn test_v1_watch_integration() {
    let mut wm = WatchManager::new();

    // Set up price drop watch
    wm.add_rule(WatchRule {
        id: "v1-price-watch".to_string(),
        domain: "shop.com".to_string(),
        model_type: Some("Product".to_string()),
        feature_dim: FEAT_PRICE as u8,
        condition: WatchCondition::ValueBelow(50.0),
        notify: NotifyTarget::EventBus,
        active: true,
        created_at: Utc::now(),
        last_triggered: None,
    });

    // Set up availability watch
    wm.add_rule(WatchRule {
        id: "v1-avail-watch".to_string(),
        domain: "shop.com".to_string(),
        model_type: Some("Product".to_string()),
        feature_dim: FEAT_AVAILABILITY as u8,
        condition: WatchCondition::Available,
        notify: NotifyTarget::EventBus,
        active: true,
        created_at: Utc::now(),
        last_triggered: None,
    });

    assert_eq!(wm.list_rules().len(), 2);

    // Simulate price drop
    let alerts = wm.evaluate("shop.com", FEAT_PRICE as u8, 40.0, 100.0);
    assert_eq!(alerts.len(), 1, "price drop should trigger alert");
    assert_eq!(alerts[0].rule_id, "v1-price-watch");

    // Simulate item becoming available
    let alerts = wm.evaluate("shop.com", FEAT_AVAILABILITY as u8, 1.0, 0.0);
    assert_eq!(alerts.len(), 1, "availability should trigger alert");

    // Check recent alerts
    let recent = wm.recent_alerts(10);
    assert_eq!(recent.len(), 2, "should have 2 recent alerts");
}

/// Test: privacy stripping before sharing
#[test]
fn test_v1_privacy_before_sharing() {
    let mut map = build_ecommerce_map("private-site.com", 5);

    // Inject session data into features
    for features in &mut map.features {
        for dim in 112..FEATURE_DIM {
            features[dim] = 42.0;
        }
    }

    strip_private_data(&mut map);

    // All session features should be zeroed
    for features in &map.features {
        for dim in 112..FEATURE_DIM {
            assert_eq!(
                features[dim], 0.0,
                "session dim {dim} must be stripped before sharing"
            );
        }
    }
}

/// Test: delta computation accuracy
#[test]
fn test_v1_delta_accuracy() {
    let map1 = build_ecommerce_map("delta-test.com", 10);
    let map2 = build_ecommerce_map("delta-test.com", 12);

    let delta = compute_delta(&map1, &map2, "test-instance");

    assert_eq!(delta.domain, "delta-test.com");
    assert_eq!(
        delta.nodes_added.len(),
        2,
        "should detect 2 new nodes (12 - 10)"
    );
    assert_eq!(delta.cortex_instance_id, "test-instance");
}

/// Test: pattern detection with realistic data
#[test]
fn test_v1_pattern_detection() {
    // Decreasing prices over time
    let now = Utc::now();
    let history: Vec<(chrono::DateTime<Utc>, f32)> = (0..10)
        .map(|i| {
            (
                now - chrono::Duration::days(10 - i),
                100.0 - (i as f32 * 5.0),
            )
        })
        .collect();

    let patterns = detect_patterns(&history);
    // Should detect a trend
    let has_trend = patterns.iter().any(|p| matches!(p, Pattern::Trend { .. }));
    assert!(
        has_trend,
        "steadily declining prices should produce a Trend pattern"
    );

    // Check the trend direction
    for p in &patterns {
        if let Pattern::Trend { direction, .. } = p {
            assert!(
                matches!(direction, TrendDirection::Decreasing),
                "trend should be Decreasing"
            );
        }
    }

    // Predict next value
    let prediction = predict(&history, 3);
    // predict returns Option<f32> for a single point
    // With 10 data points, prediction should succeed
    assert!(
        prediction.is_some(),
        "with 10 data points, prediction should succeed"
    );
    if let Some(val) = prediction {
        assert!(
            val < 55.0,
            "predicted value should continue downward trend, got {val}"
        );
    }
}

/// Test: full v1.0 pipeline — map → compile → registry → temporal → WQL
#[test]
fn test_v1_full_pipeline() {
    let dir = TempDir::new().unwrap();
    let mut registry = LocalRegistry::new(dir.path().to_path_buf()).unwrap();

    // Step 1: Create a map
    let map = build_ecommerce_map("pipeline.com", 15);
    assert!(map.nodes.len() >= 16, "map should have 16+ nodes");

    // Step 2: Compile schema
    let schema = infer_schema(&map, "pipeline.com");
    assert!(!schema.models.is_empty(), "should infer models");

    // Step 3: Generate client code
    let generated = codegen::generate_all_in_memory(&schema);
    assert!(!generated.files.is_empty(), "codegen should produce files");
    let py = generated
        .files
        .iter()
        .find(|f| f.filename == "client.py")
        .expect("should have Python client");
    assert!(py.content.contains("class"), "Python should have classes");
    let ts = generated
        .files
        .iter()
        .find(|f| f.filename == "client.ts")
        .expect("should have TypeScript client");
    assert!(
        ts.content.contains("interface") || ts.content.contains("class"),
        "TS should have types"
    );

    // Step 4: Push to registry
    registry.push("pipeline.com", &map, None).unwrap();

    // Step 5: Create second version and push with delta
    let map2 = build_ecommerce_map("pipeline.com", 18);
    let delta = compute_delta(&map, &map2, "pipeline-test");
    registry.push("pipeline.com", &map2, Some(delta)).unwrap();

    // Step 6: Verify registry state
    let stats = registry.stats();
    assert_eq!(stats.domain_count, 1);
    assert!(stats.total_snapshot_bytes > 0);
    assert_eq!(stats.total_deltas, 1);

    // Step 7: Query via WQL
    let mut maps = HashMap::new();
    maps.insert("pipeline.com".to_string(), map2);

    let query = parser::parse("SELECT * FROM Product ACROSS pipeline.com LIMIT 5").unwrap();
    let plan = planner::plan(&query, None).unwrap();
    let results = executor::execute(&plan, &maps).unwrap();
    assert!(
        !results.is_empty(),
        "WQL should find products in the pipeline"
    );

    // Step 8: Query temporal history
    let read_reg = Arc::new(LocalRegistry::new(dir.path().to_path_buf()).unwrap());
    let store = TemporalStore::new(read_reg);
    let since = Utc::now() - chrono::Duration::hours(1);
    let _history = store
        .history("pipeline.com", "/product/0", FEAT_PRICE as u8, since)
        .unwrap();
    // History query should succeed (may be empty if deltas are stored differently)
}

/// Test: multi-site compilation statistics
#[test]
fn test_v1_multi_site_compilation_stats() {
    let sites: Vec<(&str, SiteMap)> = vec![
        ("ecom1.com", build_ecommerce_map("ecom1.com", 10)),
        ("ecom2.com", build_ecommerce_map("ecom2.com", 20)),
        ("news1.com", build_news_map("news1.com", 15)),
        ("news2.com", build_news_map("news2.com", 8)),
        ("docs1.com", build_docs_map("docs1.com", 12)),
    ];

    let mut compilable = 0;
    let mut total_models = 0;
    let mut total_fields = 0;

    for (domain, map) in &sites {
        let schema = infer_schema(map, domain);
        if !schema.models.is_empty() {
            compilable += 1;
            total_models += schema.models.len();
            let fields: usize = schema.models.iter().map(|m| m.fields.len()).sum();
            total_fields += fields;
        }
    }

    assert!(
        compilable >= 4,
        "at least 4/5 sites should be compilable, got {compilable}"
    );
    assert!(
        total_models >= 5,
        "should have at least 5 total models, got {total_models}"
    );
    let avg_fields = if total_models > 0 {
        total_fields as f32 / total_models as f32
    } else {
        0.0
    };
    assert!(
        avg_fields >= 2.0,
        "average fields per model should be >= 2, got {avg_fields}"
    );
}
