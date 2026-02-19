//! Real benchmark suite for research paper data.
//! Measures file sizes, query latency, write performance, and compilation stats.
//! All numbers are reported as-measured — no fabrication.

use cortex_runtime::collective::delta;
use cortex_runtime::collective::registry::LocalRegistry;
use cortex_runtime::compiler::schema;
use cortex_runtime::map::builder::SiteMapBuilder;
use cortex_runtime::map::types::{EdgeFlags, EdgeType, PageType, PathConstraints, SiteMap};
use cortex_runtime::navigation::pathfinder;
use cortex_runtime::wql::{executor, parser, planner};
use std::collections::HashMap;
use std::time::Instant;
use tempfile::TempDir;

/// Build a synthetic e-commerce map with N product nodes.
fn build_ecommerce_map(domain: &str, num_products: usize) -> SiteMap {
    let mut builder = SiteMapBuilder::new(domain);
    let mut next_idx: u32 = 0;

    // Home page (index 0)
    let mut home_feats = [0.0f32; 128];
    home_feats[0] = 0.95;
    home_feats[80] = 1.0;
    home_feats[64] = 50.0;
    next_idx = builder.add_node(
        &format!("https://{domain}/"),
        PageType::Home,
        home_feats,
        242,
    ) + 1;

    // Category pages
    let categories = ["electronics", "clothing", "books", "home", "sports"];
    let mut cat_indices: Vec<u32> = Vec::new();
    for (i, cat) in categories.iter().enumerate() {
        let mut feats = [0.0f32; 128];
        feats[0] = 0.85;
        feats[80] = 1.0;
        feats[64] = 30.0;
        let idx = builder.add_node(
            &format!("https://{domain}/category/{cat}"),
            PageType::ProductListing,
            feats,
            217,
        );
        cat_indices.push(idx);
        builder.add_edge(0, idx, EdgeType::Navigation, 1, EdgeFlags::default());
        builder.add_edge(idx, 0, EdgeType::Navigation, 2, EdgeFlags::default());

        if i > 0 {
            builder.add_edge(idx, idx - 1, EdgeType::Navigation, 2, EdgeFlags::default());
        }
        next_idx = idx + 1;
    }

    // Product pages
    for i in 0..num_products {
        let cat_idx = cat_indices[i % categories.len()];
        let mut feats = [0.0f32; 128];
        feats[0] = 0.92;
        feats[48] = 10.0 + (i as f32 * 7.3) % 990.0;
        feats[49] = feats[48] * 1.2;
        feats[50] = 0.1 + (i as f32 * 0.03) % 0.5;
        feats[51] = if i % 10 == 0 { 0.0 } else { 1.0 };
        feats[52] = 1.0 + (i as f32 * 0.7) % 4.0;
        feats[53] = (i as f32 * 13.0) % 500.0;
        feats[80] = 1.0;
        feats[96] = 2.0;
        feats[16] = 500.0 + (i as f32 * 23.0) % 2000.0;
        feats[17] = 3.0;
        feats[18] = 2.0 + (i as f32 * 0.5) % 10.0;
        let idx = builder.add_node(
            &format!("https://{domain}/product/{i}"),
            PageType::ProductDetail,
            feats,
            235,
        );
        builder.add_edge(cat_idx, idx, EdgeType::Navigation, 1, EdgeFlags::default());
        builder.add_edge(idx, cat_idx, EdgeType::Navigation, 2, EdgeFlags::default());
        builder.add_edge(0, idx, EdgeType::ContentLink, 3, EdgeFlags::default());

        if i > 3 {
            let related = cat_indices[(i + 1) % categories.len()];
            builder.add_edge(idx, related, EdgeType::Related, 2, EdgeFlags::default());
        }
        next_idx = idx + 1;
    }

    // Cart + Checkout
    let mut cart_feats = [0.0f32; 128];
    cart_feats[0] = 0.9;
    cart_feats[80] = 1.0;
    cart_feats[112] = 1.0;
    let cart_idx = builder.add_node(
        &format!("https://{domain}/cart"),
        PageType::Cart,
        cart_feats,
        230,
    );
    builder.add_edge(0, cart_idx, EdgeType::Navigation, 1, EdgeFlags::default());

    let mut checkout_feats = [0.0f32; 128];
    checkout_feats[0] = 0.88;
    checkout_feats[80] = 1.0;
    checkout_feats[85] = 1.0;
    let checkout_idx = builder.add_node(
        &format!("https://{domain}/checkout"),
        PageType::Checkout,
        checkout_feats,
        224,
    );
    builder.add_edge(
        cart_idx,
        checkout_idx,
        EdgeType::Navigation,
        1,
        EdgeFlags::default(),
    );

    builder.build()
}

#[test]
fn bench_file_size_scaling() {
    println!("\n=== FILE SIZE SCALING ===\n");
    println!(
        "{:<12} {:<10} {:<15} {:<15} {:<10}",
        "Nodes", "Edges", "Raw JSON (est)", ".ctx Size", "Ratio"
    );
    println!("{}", "-".repeat(62));

    for &count in &[100, 500, 1_000, 5_000, 10_000] {
        let map = build_ecommerce_map("bench.com", count);
        let data = map.serialize();
        let ctx_size = data.len();

        let raw_estimate =
            map.nodes.len() * 300 + map.edges.len() * 80 + map.features.len() * 128 * 8;
        let ratio = raw_estimate as f64 / ctx_size as f64;

        println!(
            "{:<12} {:<10} {:<15} {:<15} {:.1}x",
            map.nodes.len(),
            map.edges.len(),
            format_bytes(raw_estimate),
            format_bytes(ctx_size),
            ratio
        );

        let restored = SiteMap::deserialize(&data).unwrap();
        assert_eq!(restored.nodes.len(), map.nodes.len());
    }
}

#[test]
fn bench_serialize_deserialize() {
    println!("\n=== SERIALIZE/DESERIALIZE PERFORMANCE ===\n");
    println!(
        "{:<12} {:<15} {:<15} {:<15}",
        "Nodes", "Serialize", "Deserialize", "File Size"
    );
    println!("{}", "-".repeat(57));

    for &count in &[100, 1_000, 5_000, 10_000] {
        let map = build_ecommerce_map("bench.com", count);

        let start = Instant::now();
        let mut data = Vec::new();
        for _ in 0..10 {
            data = map.serialize();
        }
        let serialize_us = start.elapsed().as_micros() / 10;

        let start = Instant::now();
        for _ in 0..10 {
            let _ = SiteMap::deserialize(&data).unwrap();
        }
        let deserialize_us = start.elapsed().as_micros() / 10;

        println!(
            "{:<12} {:<15} {:<15} {:<15}",
            map.nodes.len(),
            format!("{} us", serialize_us),
            format!("{} us", deserialize_us),
            format_bytes(data.len()),
        );
    }
}

#[test]
fn bench_query_performance() {
    println!("\n=== QUERY PERFORMANCE ===\n");
    println!(
        "{:<12} {:<15} {:<15} {:<15} {:<15}",
        "Nodes", "Filter Type", "Filter+Feat", "Pathfind", "Similarity"
    );
    println!("{}", "-".repeat(72));

    for &count in &[100, 1_000, 5_000, 10_000] {
        let map = build_ecommerce_map("bench.com", count);

        // Filter by page type (100 iterations)
        let start = Instant::now();
        for _ in 0..100 {
            let results: Vec<_> = map
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.page_type == PageType::ProductDetail)
                .take(20)
                .collect();
            assert!(!results.is_empty());
        }
        let filter_type_us = start.elapsed().as_micros() / 100;

        // Filter by type + feature range (price < 500)
        let start = Instant::now();
        for _ in 0..100 {
            let results: Vec<_> = map
                .nodes
                .iter()
                .enumerate()
                .filter(|(i, n)| {
                    n.page_type == PageType::ProductDetail
                        && map.features.get(*i).map_or(false, |f| f[48] < 500.0)
                })
                .take(20)
                .collect();
            assert!(!results.is_empty());
        }
        let filter_feat_us = start.elapsed().as_micros() / 100;

        // Pathfind (100 iterations)
        let target = (map.nodes.len().min(50) - 1) as u32;
        let start = Instant::now();
        for _ in 0..100 {
            let _ = pathfinder::find_path(&map, 0, target, &PathConstraints::default());
        }
        let pathfind_us = start.elapsed().as_micros() / 100;

        // Similarity search (cosine, top-10)
        let query_vec = &map.features[6 % map.features.len()];
        let start = Instant::now();
        for _ in 0..100 {
            let mut scores: Vec<(usize, f32)> = map
                .features
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    let dot: f32 = f.iter().zip(query_vec.iter()).map(|(a, b)| a * b).sum();
                    let norm_a: f32 = f.iter().map(|x| x * x).sum::<f32>().sqrt();
                    let norm_b: f32 = query_vec.iter().map(|x| x * x).sum::<f32>().sqrt();
                    let sim = if norm_a * norm_b > 0.0 {
                        dot / (norm_a * norm_b)
                    } else {
                        0.0
                    };
                    (i, sim)
                })
                .collect();
            scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            scores.truncate(10);
            assert!(!scores.is_empty());
        }
        let similarity_us = start.elapsed().as_micros() / 100;

        println!(
            "{:<12} {:<15} {:<15} {:<15} {:<15}",
            map.nodes.len(),
            format!("{} us", filter_type_us),
            format!("{} us", filter_feat_us),
            format!("{} us", pathfind_us),
            format!("{} us", similarity_us),
        );
    }
}

#[test]
fn bench_write_performance() {
    println!("\n=== WRITE PERFORMANCE ===\n");
    println!(
        "{:<12} {:<15} {:<15} {:<18} {:<15} {:<15}",
        "Base Nodes", "Add Node", "Add Edge", "Batch 100 Nodes", "File Write", "File Read"
    );
    println!("{}", "-".repeat(90));

    for &base_count in &[100, 1_000, 5_000] {
        let map = build_ecommerce_map("bench.com", base_count);

        // Measure add_node (1000 iterations)
        let start = Instant::now();
        for _ in 0..1000 {
            let mut b = SiteMapBuilder::new("bench.com");
            b.add_node(
                "https://bench.com/x",
                PageType::ProductDetail,
                [0.0; 128],
                200,
            );
        }
        let add_node_ns = start.elapsed().as_nanos() / 1000;

        // Measure add_edge
        let start = Instant::now();
        for _ in 0..1000 {
            let mut b = SiteMapBuilder::new("bench.com");
            b.add_node("https://bench.com/a", PageType::Home, [0.0; 128], 200);
            b.add_node(
                "https://bench.com/b",
                PageType::ProductDetail,
                [0.0; 128],
                200,
            );
            b.add_edge(0, 1, EdgeType::Navigation, 1, EdgeFlags::default());
        }
        let add_edge_ns = start.elapsed().as_nanos() / 1000;

        // Batch 100 nodes
        let start = Instant::now();
        for _ in 0..100 {
            let mut b = SiteMapBuilder::new("bench.com");
            for j in 0..100u32 {
                b.add_node(
                    &format!("https://bench.com/p/{j}"),
                    PageType::ProductDetail,
                    [0.0; 128],
                    200,
                );
            }
            let _ = b.build();
        }
        let batch_us = start.elapsed().as_micros() / 100;

        // File write
        let data = map.serialize();
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bench.ctx");
        let start = Instant::now();
        for _ in 0..100 {
            std::fs::write(&path, &data).unwrap();
        }
        let write_us = start.elapsed().as_micros() / 100;

        // File read + deserialize
        let start = Instant::now();
        for _ in 0..100 {
            let bytes = std::fs::read(&path).unwrap();
            let _ = SiteMap::deserialize(&bytes).unwrap();
        }
        let read_us = start.elapsed().as_micros() / 100;

        println!(
            "{:<12} {:<15} {:<15} {:<18} {:<15} {:<15}",
            map.nodes.len(),
            format!("{} ns", add_node_ns),
            format!("{} ns", add_edge_ns),
            format!("{} us", batch_us),
            format!("{} us", write_us),
            format!("{} us", read_us),
        );
    }
}

#[test]
fn bench_wql_performance() {
    println!("\n=== WQL QUERY PERFORMANCE ===\n");
    println!(
        "{:<12} {:<15} {:<18} {:<18}",
        "Nodes", "Parse", "Plan+Execute", "Full Pipeline"
    );
    println!("{}", "-".repeat(63));

    for &count in &[100, 1_000, 5_000, 10_000] {
        let map = build_ecommerce_map("bench.com", count);
        let mut maps: HashMap<String, SiteMap> = HashMap::new();
        maps.insert("bench.com".to_string(), map);

        let query =
            "SELECT url, page_type FROM ProductDetail WHERE price < 500 ORDER BY price ASC LIMIT 20";

        // Parse (1000 iterations)
        let start = Instant::now();
        for _ in 0..1000 {
            let _ = parser::parse(query).unwrap();
        }
        let parse_us = start.elapsed().as_micros() / 1000;

        // Plan + Execute (100 iterations)
        let ast = parser::parse(query).unwrap();
        let plan = planner::plan(&ast, None).unwrap();
        let start = Instant::now();
        for _ in 0..100 {
            let _ = executor::execute(&plan, &maps);
        }
        let exec_us = start.elapsed().as_micros() / 100;

        // Full pipeline (100 iterations)
        let start = Instant::now();
        for _ in 0..100 {
            let ast = parser::parse(query).unwrap();
            let plan = planner::plan(&ast, None).unwrap();
            let _ = executor::execute(&plan, &maps);
        }
        let full_us = start.elapsed().as_micros() / 100;

        println!(
            "{:<12} {:<15} {:<18} {:<18}",
            count + 7,
            format!("{} us", parse_us),
            format!("{} us", exec_us),
            format!("{} us", full_us),
        );
    }
}

#[test]
fn bench_compiler_performance() {
    println!("\n=== WEB COMPILER PERFORMANCE ===\n");
    println!(
        "{:<12} {:<10} {:<10} {:<10} {:<15}",
        "Nodes", "Models", "Fields", "Relations", "Schema Infer"
    );
    println!("{}", "-".repeat(57));

    for &count in &[100, 1_000, 5_000, 10_000] {
        let map = build_ecommerce_map("bench.com", count);

        let start = Instant::now();
        let compiled = schema::infer_schema(&map, "bench.com");
        let infer_us = start.elapsed().as_micros();

        println!(
            "{:<12} {:<10} {:<10} {:<10} {:<15}",
            map.nodes.len(),
            compiled.models.len(),
            compiled
                .models
                .iter()
                .map(|m| m.fields.len())
                .sum::<usize>(),
            compiled.relationships.len(),
            format!("{} us", infer_us),
        );
    }
}

#[test]
fn bench_delta_performance() {
    println!("\n=== DELTA COMPUTATION PERFORMANCE ===\n");
    println!(
        "{:<12} {:<15} {:<18}",
        "Nodes", "Compute Delta", "Delta Size"
    );
    println!("{}", "-".repeat(45));

    for &count in &[100, 1_000, 5_000] {
        let old_map = build_ecommerce_map("bench.com", count);
        let new_map = build_ecommerce_map("bench.com", count + count / 10);

        let start = Instant::now();
        let d = delta::compute_delta(&old_map, &new_map, "bench");
        let delta_us = start.elapsed().as_micros();

        let delta_bytes = delta::serialize_delta(&d);

        println!(
            "{:<12} {:<15} {:<18}",
            old_map.nodes.len(),
            format!("{} us", delta_us),
            format_bytes(delta_bytes.len()),
        );
    }
}

#[test]
fn bench_privacy_strip() {
    println!("\n=== PRIVACY STRIP PERFORMANCE ===\n");

    for &count in &[100, 1_000, 5_000] {
        let mut map = build_ecommerce_map("bench.com", count);

        let start = Instant::now();
        delta::strip_private_data(&mut map);
        let strip_us = start.elapsed().as_micros();

        println!("Strip {} nodes: {} us", count + 7, strip_us);
    }
}

#[test]
fn bench_registry_performance() {
    println!("\n=== REGISTRY PERFORMANCE ===\n");

    let dir = TempDir::new().unwrap();
    let mut registry = LocalRegistry::new(dir.path().to_path_buf()).unwrap();

    let map = build_ecommerce_map("bench.com", 1000);

    let start = Instant::now();
    registry.push("bench.com", &map, None).unwrap();
    let push_us = start.elapsed().as_micros();

    let start = Instant::now();
    let (pulled, _) = registry.pull("bench.com").unwrap().unwrap();
    let pull_us = start.elapsed().as_micros();

    assert_eq!(pulled.nodes.len(), map.nodes.len());

    println!("Push (1007 nodes): {} us", push_us);
    println!("Pull (1007 nodes): {} us", pull_us);

    let start = Instant::now();
    for i in 0..10 {
        let m = build_ecommerce_map(&format!("site{i}.com"), 100);
        registry.push(&format!("site{i}.com"), &m, None).unwrap();
    }
    let multi_push_us = start.elapsed().as_micros();
    println!(
        "Push 10 domains (107 nodes each): {} us total",
        multi_push_us
    );

    let stats = registry.stats();
    println!(
        "Registry: {} domains, {} bytes total",
        stats.domain_count, stats.total_snapshot_bytes
    );
}

#[test]
fn bench_real_map_stats() {
    println!("\n=== REAL MAP STATISTICS ===\n");

    let cache_dir = dirs::home_dir().unwrap().join(".cortex").join("maps");

    if !cache_dir.exists() {
        println!("No cached maps found at {:?}", cache_dir);
        return;
    }

    for entry in std::fs::read_dir(&cache_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "ctx") {
            let data = std::fs::read(&path).unwrap();
            let map = SiteMap::deserialize(&data).unwrap();
            let domain = &map.header.domain;

            let rendered = map.nodes.iter().filter(|n| n.flags.is_rendered()).count();
            let product_count = map
                .nodes
                .iter()
                .filter(|n| n.page_type == PageType::ProductDetail)
                .count();

            println!("Domain: {}", domain);
            println!(
                "  File size:  {} bytes ({:.1} KB)",
                data.len(),
                data.len() as f64 / 1024.0
            );
            println!("  Nodes:      {}", map.nodes.len());
            println!("  Edges:      {}", map.edges.len());
            println!("  Actions:    {}", map.actions.len());
            println!("  Clusters:   {}", map.cluster_centroids.len());
            println!("  Rendered:   {}", rendered);
            println!("  Products:   {}", product_count);
            println!(
                "  Features:   {} vectors x 128 dims = {} floats",
                map.features.len(),
                map.features.len() * 128
            );
            println!(
                "  Bytes/node: {:.1}",
                data.len() as f64 / map.nodes.len() as f64
            );
            println!();
        }
    }
}

fn format_bytes(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
