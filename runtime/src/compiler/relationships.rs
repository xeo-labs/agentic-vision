//! Relationship extraction — discovers connections between data models from graph edges.
//!
//! Analyzes edge patterns between typed nodes to infer belongs_to, has_many,
//! has_one, and many_to_many relationships.

use crate::compiler::models::*;
use crate::map::types::*;
use std::collections::HashMap;

/// Infer relationships between data models from SiteMap edges.
///
/// For each edge, looks up the source and target node types. If they map to
/// different models, records a relationship. Deduplicates, classifies cardinality,
/// and names the relationship.
pub fn infer_relationships(site_map: &SiteMap, models: &[DataModel]) -> Vec<ModelRelationship> {
    // Build node_index → model name lookup
    let mut node_to_model: HashMap<usize, &str> = HashMap::new();
    for model in models {
        for (idx, node) in site_map.nodes.iter().enumerate() {
            if let Some(schema_type) = page_type_to_schema_org_name(node.page_type) {
                if schema_type == model.schema_org_type {
                    node_to_model.insert(idx, &model.name);
                }
            }
        }
    }

    // Count edges between model types
    // Key: (from_model, to_model, edge_type) → count
    let mut edge_counts: HashMap<(String, String, EdgeType), usize> = HashMap::new();
    // Track per-source-node outbound counts for cardinality inference
    let mut per_node_outbound: HashMap<(String, String, usize), usize> = HashMap::new();

    for (src_idx, _node) in site_map.nodes.iter().enumerate() {
        let src_model = match node_to_model.get(&src_idx) {
            Some(m) => *m,
            None => continue,
        };

        // Get edges for this node using CSR index
        let edge_start = if src_idx < site_map.edge_index.len() {
            site_map.edge_index[src_idx] as usize
        } else {
            continue;
        };
        let edge_end = if src_idx + 1 < site_map.edge_index.len() {
            site_map.edge_index[src_idx + 1] as usize
        } else {
            site_map.edges.len()
        };

        for edge_idx in edge_start..edge_end {
            if edge_idx >= site_map.edges.len() {
                break;
            }
            let edge = &site_map.edges[edge_idx];
            let target_idx = edge.target_node as usize;

            let target_model = match node_to_model.get(&target_idx) {
                Some(m) => *m,
                None => continue,
            };

            // Skip self-type relationships unless they're "Related" edges
            if src_model == target_model && edge.edge_type != EdgeType::Related {
                continue;
            }

            let key = (
                src_model.to_string(),
                target_model.to_string(),
                edge.edge_type,
            );
            *edge_counts.entry(key).or_insert(0) += 1;

            let node_key = (src_model.to_string(), target_model.to_string(), src_idx);
            *per_node_outbound.entry(node_key).or_insert(0) += 1;
        }
    }

    // Build relationships from edge counts
    let mut relationships: Vec<ModelRelationship> = Vec::new();
    let mut seen: HashMap<(String, String), bool> = HashMap::new();

    for ((from_model, to_model, edge_type), count) in &edge_counts {
        // Skip if we've already seen this model pair
        let pair_key = (from_model.clone(), to_model.clone());
        if seen.contains_key(&pair_key) {
            continue;
        }
        seen.insert(pair_key, true);

        // Determine cardinality
        let cardinality = infer_cardinality(
            from_model,
            to_model,
            *count,
            &per_node_outbound,
            &edge_counts,
        );

        // Generate relationship name
        let name = generate_relationship_name(from_model, to_model, &cardinality, *edge_type);

        // Build traversal hint
        let traversal_hint = TraversalHint {
            edge_types: vec![format!("{edge_type:?}")],
            forward: true,
        };

        relationships.push(ModelRelationship {
            from_model: from_model.clone(),
            to_model: to_model.clone(),
            name,
            cardinality,
            edge_count: *count,
            traversal_hint,
        });
    }

    // Sort by edge count (most significant first)
    relationships.sort_by(|a, b| b.edge_count.cmp(&a.edge_count));

    relationships
}

/// Infer cardinality from edge patterns.
fn infer_cardinality(
    from_model: &str,
    to_model: &str,
    _total_count: usize,
    per_node: &HashMap<(String, String, usize), usize>,
    edge_counts: &HashMap<(String, String, EdgeType), usize>,
) -> Cardinality {
    // Count how many edges each source node has to the target model
    let outbound_counts: Vec<usize> = per_node
        .iter()
        .filter(|((f, t, _), _)| f == from_model && t == to_model)
        .map(|(_, &count)| count)
        .collect();

    if outbound_counts.is_empty() {
        return Cardinality::HasMany;
    }

    let avg_outbound = outbound_counts.iter().sum::<usize>() as f64 / outbound_counts.len() as f64;
    let max_outbound = *outbound_counts.iter().max().unwrap_or(&1);

    // Check reverse direction
    let reverse_count: usize = edge_counts
        .iter()
        .filter(|((f, t, _), _)| f == to_model && t == from_model)
        .map(|(_, &c)| c)
        .sum();

    // Same type → many_to_many (e.g., similar products)
    if from_model == to_model {
        return Cardinality::ManyToMany;
    }

    // If average outbound is ~1 → belongs_to
    if avg_outbound <= 1.2 && max_outbound <= 2 {
        return Cardinality::BelongsTo;
    }

    // If reverse direction also has many edges → many_to_many
    if reverse_count > 0 && avg_outbound > 2.0 {
        return Cardinality::ManyToMany;
    }

    // If one → one
    if max_outbound == 1 && avg_outbound == 1.0 {
        return Cardinality::HasOne;
    }

    // Default: has_many
    Cardinality::HasMany
}

/// Generate a human-readable relationship name.
fn generate_relationship_name(
    from: &str,
    to: &str,
    cardinality: &Cardinality,
    edge_type: EdgeType,
) -> String {
    // Special naming for well-known patterns
    match (from, to) {
        ("Product", "Category") => return "belongs_to_category".to_string(),
        ("Product", "Product") => return "similar_to".to_string(),
        ("Product", "Review") => return "has_reviews".to_string(),
        ("Article", "Article") => return "related_articles".to_string(),
        ("Review", "Product") => return "reviews_product".to_string(),
        _ => {}
    }

    // Generic naming based on cardinality
    let to_snake = to_snake_case(to);
    match cardinality {
        Cardinality::BelongsTo => format!("belongs_to_{to_snake}"),
        Cardinality::HasMany => format!("has_{to_snake}s"),
        Cardinality::HasOne => format!("has_{to_snake}"),
        Cardinality::ManyToMany => {
            if edge_type == EdgeType::Related {
                format!("related_{to_snake}s")
            } else {
                format!("{to_snake}s")
            }
        }
    }
}

/// Convert PascalCase to snake_case.
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(c.to_lowercase().next().unwrap_or(c));
    }
    result
}

/// Map PageType to Schema.org type name (mirrors the schema.rs mapping).
fn page_type_to_schema_org_name(pt: PageType) -> Option<&'static str> {
    match pt {
        PageType::ProductDetail => Some("Product"),
        PageType::ProductListing => Some("ProductListing"),
        PageType::Article => Some("Article"),
        PageType::ReviewList => Some("Review"),
        PageType::Faq => Some("FAQPage"),
        PageType::AboutPage => Some("Organization"),
        PageType::ContactPage => Some("ContactPoint"),
        PageType::PricingPage => Some("Offer"),
        PageType::Documentation => Some("TechArticle"),
        PageType::MediaPage => Some("MediaObject"),
        PageType::Forum => Some("DiscussionForumPosting"),
        PageType::SocialFeed => Some("SocialMediaPosting"),
        PageType::Calendar => Some("Event"),
        PageType::Cart => Some("Cart"),
        PageType::Checkout => Some("CheckoutPage"),
        PageType::Account => Some("Account"),
        PageType::Login => Some("LoginPage"),
        PageType::Home => Some("WebSite"),
        PageType::SearchResults => Some("SearchResultsPage"),
        PageType::Dashboard => Some("Dashboard"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::builder::SiteMapBuilder;

    #[test]
    fn test_infer_relationships_basic() {
        let mut builder = SiteMapBuilder::new("test.com");

        // Category (ProductListing)
        let feats = [0.0f32; FEATURE_DIM];
        builder.add_node(
            "https://test.com/electronics",
            PageType::ProductListing,
            feats,
            200,
        );
        builder.add_node(
            "https://test.com/electronics2",
            PageType::ProductListing,
            feats,
            200,
        );

        // Products
        for i in 0..5 {
            let mut pf = [0.0f32; FEATURE_DIM];
            pf[FEAT_PRICE] = 100.0;
            builder.add_node(
                &format!("https://test.com/product/{i}"),
                PageType::ProductDetail,
                pf,
                200,
            );
            // Category → Product edge
            builder.add_edge(0, 2 + i, EdgeType::ContentLink, 1, EdgeFlags::default());
        }

        let map = builder.build();
        let models = vec![
            DataModel {
                name: "Category".to_string(),
                schema_org_type: "ProductListing".to_string(),
                fields: vec![],
                instance_count: 2,
                example_urls: vec![],
                search_action: None,
                list_url: None,
            },
            DataModel {
                name: "Product".to_string(),
                schema_org_type: "Product".to_string(),
                fields: vec![],
                instance_count: 5,
                example_urls: vec![],
                search_action: None,
                list_url: None,
            },
        ];

        let rels = infer_relationships(&map, &models);
        assert!(!rels.is_empty(), "should discover relationships");

        // Should find Category → Product
        let cat_product = rels
            .iter()
            .find(|r| r.from_model == "Category" && r.to_model == "Product");
        assert!(cat_product.is_some(), "should find Category → Product");
    }

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("Product"), "product");
        assert_eq!(to_snake_case("ProductDetail"), "product_detail");
        assert_eq!(to_snake_case("FAQ"), "f_a_q");
        assert_eq!(to_snake_case("Cart"), "cart");
    }

    #[test]
    fn test_generate_relationship_name() {
        assert_eq!(
            generate_relationship_name(
                "Product",
                "Category",
                &Cardinality::BelongsTo,
                EdgeType::Breadcrumb
            ),
            "belongs_to_category"
        );
        assert_eq!(
            generate_relationship_name(
                "Product",
                "Product",
                &Cardinality::ManyToMany,
                EdgeType::Related
            ),
            "similar_to"
        );
        assert_eq!(
            generate_relationship_name(
                "Product",
                "Review",
                &Cardinality::HasMany,
                EdgeType::ContentLink
            ),
            "has_reviews"
        );
    }

    #[test]
    fn test_empty_relationships() {
        let builder = SiteMapBuilder::new("empty.com");
        let map = builder.build();
        let rels = infer_relationships(&map, &[]);
        assert!(rels.is_empty());
    }
}
