//! Schema inference engine — discovers typed data models from SiteMap structured data.
//!
//! Walks every node in the SiteMap, groups by Schema.org type, infers field types,
//! calculates nullability, and produces a `CompiledSchema` with all discovered models.

use crate::compiler::actions::compile_actions;
use crate::compiler::models::*;
use crate::compiler::relationships::infer_relationships;
use crate::map::types::*;
use chrono::Utc;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Page type to Schema.org type name mapping.
fn page_type_to_schema_org(pt: PageType) -> Option<&'static str> {
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

/// Infer field name and type from a feature dimension for a given page type.
fn feature_dim_to_field(dim: usize, value: f32, pt: PageType) -> Option<(String, FieldType, f32)> {
    // Only return fields relevant to this page type with meaningful values
    if value == 0.0 {
        return None;
    }

    match dim {
        // Commerce fields — primarily for product pages
        FEAT_PRICE if pt == PageType::ProductDetail || pt == PageType::PricingPage => {
            Some(("price".to_string(), FieldType::Float, value))
        }
        FEAT_PRICE_ORIGINAL if pt == PageType::ProductDetail => {
            Some(("original_price".to_string(), FieldType::Float, value))
        }
        FEAT_DISCOUNT_PCT if pt == PageType::ProductDetail => {
            Some(("discount_percent".to_string(), FieldType::Float, value))
        }
        FEAT_AVAILABILITY if pt == PageType::ProductDetail => Some((
            "availability".to_string(),
            FieldType::Enum(vec![
                "in_stock".to_string(),
                "out_of_stock".to_string(),
                "preorder".to_string(),
            ]),
            value,
        )),
        FEAT_RATING
            if pt == PageType::ProductDetail
                || pt == PageType::ReviewList
                || pt == PageType::Article =>
        {
            Some(("rating".to_string(), FieldType::Float, value))
        }
        FEAT_REVIEW_COUNT_LOG if pt == PageType::ProductDetail || pt == PageType::ReviewList => {
            Some(("review_count".to_string(), FieldType::Integer, value))
        }
        FEAT_REVIEW_SENTIMENT if pt == PageType::ProductDetail || pt == PageType::ReviewList => {
            Some(("review_sentiment".to_string(), FieldType::Float, value))
        }
        FEAT_SHIPPING_FREE if pt == PageType::ProductDetail => {
            Some(("free_shipping".to_string(), FieldType::Bool, value))
        }
        FEAT_SHIPPING_SPEED if pt == PageType::ProductDetail => {
            Some(("shipping_speed_days".to_string(), FieldType::Integer, value))
        }
        FEAT_SELLER_REPUTATION if pt == PageType::ProductDetail => {
            Some(("seller_reputation".to_string(), FieldType::Float, value))
        }
        FEAT_VARIANT_COUNT if pt == PageType::ProductDetail => {
            Some(("variant_count".to_string(), FieldType::Integer, value))
        }
        FEAT_DEAL_SCORE if pt == PageType::ProductDetail => {
            Some(("deal_score".to_string(), FieldType::Float, value))
        }
        FEAT_CATEGORY_PRICE_PERCENTILE if pt == PageType::ProductDetail => Some((
            "category_price_percentile".to_string(),
            FieldType::Float,
            value,
        )),
        // Content fields — for articles/docs
        FEAT_TEXT_LENGTH_LOG if pt == PageType::Article || pt == PageType::Documentation => {
            Some(("word_count".to_string(), FieldType::Integer, value))
        }
        FEAT_READING_LEVEL if pt == PageType::Article || pt == PageType::Documentation => {
            Some(("reading_level".to_string(), FieldType::Float, value))
        }
        FEAT_SENTIMENT if pt == PageType::Article => {
            Some(("sentiment".to_string(), FieldType::Float, value))
        }
        FEAT_IMAGE_COUNT => Some(("image_count".to_string(), FieldType::Integer, value)),
        FEAT_VIDEO_PRESENT if value > 0.5 => {
            Some(("has_video".to_string(), FieldType::Bool, value))
        }
        // Navigation
        FEAT_BREADCRUMB_DEPTH if value > 0.0 => {
            Some(("breadcrumb_depth".to_string(), FieldType::Integer, value))
        }
        _ => None,
    }
}

/// Infer a `CompiledSchema` from a SiteMap.
///
/// Walks every node, groups by page type / Schema.org type, infers fields,
/// discovers relationships and actions, and returns the complete compiled schema.
pub fn infer_schema(site_map: &SiteMap, domain: &str) -> CompiledSchema {
    // Group nodes by their Schema.org type
    let mut type_groups: HashMap<String, Vec<usize>> = HashMap::new();

    for (idx, node) in site_map.nodes.iter().enumerate() {
        // Only consider nodes with reasonable confidence
        let confidence = node.confidence as f32 / 255.0;
        if confidence < 0.3 {
            continue;
        }

        if let Some(schema_type) = page_type_to_schema_org(node.page_type) {
            type_groups
                .entry(schema_type.to_string())
                .or_default()
                .push(idx);
        }
    }

    // Build models from grouped nodes
    let mut models: Vec<DataModel> = Vec::new();

    for (schema_type, node_indices) in &type_groups {
        if node_indices.is_empty() {
            continue;
        }

        // Skip types with too few instances (likely noise) unless it's a singleton type
        let singleton_types = [
            "Cart",
            "CheckoutPage",
            "Account",
            "LoginPage",
            "WebSite",
            "SearchResultsPage",
            "Dashboard",
        ];
        if node_indices.len() < 2 && !singleton_types.contains(&schema_type.as_str()) {
            continue;
        }

        // Collect field data across all instances
        let mut field_occurrences: BTreeMap<String, Vec<(FieldType, f32, String)>> =
            BTreeMap::new();
        let mut example_urls: Vec<String> = Vec::new();
        let mut list_url: Option<String> = None;

        // Determine the representative PageType for this group
        let representative_pt = site_map.nodes[node_indices[0]].page_type;

        for &idx in node_indices {
            // Collect example URLs (first 5)
            if example_urls.len() < 5 && idx < site_map.urls.len() {
                example_urls.push(site_map.urls[idx].clone());
            }

            // Always include url and node_id fields
            if !field_occurrences.contains_key("url") {
                field_occurrences.insert(
                    "url".to_string(),
                    vec![(FieldType::Url, 1.0, String::new())],
                );
            }

            // Extract fields from feature vector
            if idx < site_map.features.len() {
                let features = &site_map.features[idx];
                for (dim, &value) in features.iter().enumerate() {
                    if let Some((field_name, field_type, val)) =
                        feature_dim_to_field(dim, value, representative_pt)
                    {
                        let example = format_feature_value(dim, val);
                        field_occurrences.entry(field_name).or_default().push((
                            field_type,
                            FieldSource::Inferred.default_confidence(),
                            example,
                        ));
                    }
                }

                // Check for structured data richness → implies JSON-LD fields
                if features[FEAT_HAS_STRUCTURED_DATA] > 0.5 {
                    // Add standard Schema.org fields based on type
                    add_schema_org_fields(schema_type, &mut field_occurrences);
                }
            }
        }

        // Detect listing page for this type
        for (idx, node) in site_map.nodes.iter().enumerate() {
            if node.page_type == PageType::ProductListing
                && schema_type == "Product"
                && idx < site_map.urls.len()
            {
                list_url = Some(site_map.urls[idx].clone());
                break;
            }
        }

        // Build model fields from occurrences
        let total_instances = node_indices.len();
        let mut fields: Vec<ModelField> = Vec::new();

        // Always add url and node_id as first fields
        fields.push(ModelField {
            name: "url".to_string(),
            field_type: FieldType::Url,
            source: FieldSource::Inferred,
            confidence: 1.0,
            nullable: false,
            example_values: example_urls.iter().take(3).cloned().collect(),
            feature_dim: None,
        });

        fields.push(ModelField {
            name: "node_id".to_string(),
            field_type: FieldType::Integer,
            source: FieldSource::Inferred,
            confidence: 1.0,
            nullable: false,
            example_values: node_indices.iter().take(3).map(|i| i.to_string()).collect(),
            feature_dim: None,
        });

        // Add "name" field for all types (inferred from structured data or title)
        fields.push(ModelField {
            name: "name".to_string(),
            field_type: FieldType::String,
            source: FieldSource::JsonLd,
            confidence: 0.95,
            nullable: false,
            example_values: Vec::new(),
            feature_dim: None,
        });

        for (field_name, occurrences) in &field_occurrences {
            if field_name == "url" {
                continue; // Already added
            }

            // Use the most common type
            let field_type = occurrences[0].0.clone();

            // Calculate confidence (average)
            let avg_confidence =
                occurrences.iter().map(|o| o.1).sum::<f32>() / occurrences.len() as f32;

            // Calculate nullability
            let nullable = occurrences.len() < total_instances;

            // Collect unique example values (first 5)
            let mut example_values: Vec<String> = Vec::new();
            let mut seen: BTreeSet<String> = BTreeSet::new();
            for o in occurrences {
                if !o.2.is_empty() && seen.insert(o.2.clone()) {
                    example_values.push(o.2.clone());
                    if example_values.len() >= 5 {
                        break;
                    }
                }
            }

            // Map field name to feature dimension
            let feature_dim = field_name_to_dim(field_name);

            fields.push(ModelField {
                name: field_name.clone(),
                field_type,
                source: FieldSource::Inferred,
                confidence: avg_confidence,
                nullable,
                example_values,
                feature_dim,
            });
        }

        let model_name = simplify_model_name(schema_type);

        models.push(DataModel {
            name: model_name,
            schema_org_type: schema_type.clone(),
            fields,
            instance_count: total_instances,
            example_urls,
            search_action: None,
            list_url,
        });
    }

    // Sort models by instance count (descending) for better UX
    models.sort_by(|a, b| b.instance_count.cmp(&a.instance_count));

    // Infer relationships between models
    let relationships = infer_relationships(site_map, &models);

    // Compile actions
    let actions = compile_actions(site_map, &models);

    // Attach search actions to models
    for action in &actions {
        if action.name == "search" || action.name.ends_with("_search") {
            for model in &mut models {
                if action.belongs_to == model.name && model.search_action.is_none() {
                    model.search_action = Some(action.clone());
                }
            }
        }
    }

    // Compute stats
    let total_fields: usize = models.iter().map(|m| m.fields.len()).sum();
    let total_instances: usize = models.iter().map(|m| m.instance_count).sum();
    let avg_confidence = if total_fields > 0 {
        models
            .iter()
            .flat_map(|m| m.fields.iter().map(|f| f.confidence))
            .sum::<f32>()
            / total_fields as f32
    } else {
        0.0
    };

    CompiledSchema {
        domain: domain.to_string(),
        compiled_at: Utc::now(),
        models: models.clone(),
        actions,
        relationships,
        stats: SchemaStats {
            total_models: models.len(),
            total_fields,
            total_instances,
            avg_confidence,
        },
    }
}

/// Simplify Schema.org type names to cleaner model names.
fn simplify_model_name(schema_type: &str) -> String {
    match schema_type {
        "FAQPage" => "FAQ".to_string(),
        "TechArticle" => "Article".to_string(),
        "MediaObject" => "Media".to_string(),
        "DiscussionForumPosting" => "ForumPost".to_string(),
        "SocialMediaPosting" => "SocialPost".to_string(),
        "ContactPoint" => "Contact".to_string(),
        "CheckoutPage" => "Checkout".to_string(),
        "LoginPage" => "Auth".to_string(),
        "WebSite" => "Site".to_string(),
        "SearchResultsPage" => "SearchResults".to_string(),
        "ProductListing" => "Category".to_string(),
        other => other.to_string(),
    }
}

/// Map field names back to feature vector dimensions.
fn field_name_to_dim(name: &str) -> Option<usize> {
    match name {
        "price" => Some(FEAT_PRICE),
        "original_price" => Some(FEAT_PRICE_ORIGINAL),
        "discount_percent" => Some(FEAT_DISCOUNT_PCT),
        "availability" => Some(FEAT_AVAILABILITY),
        "rating" => Some(FEAT_RATING),
        "review_count" => Some(FEAT_REVIEW_COUNT_LOG),
        "review_sentiment" => Some(FEAT_REVIEW_SENTIMENT),
        "free_shipping" => Some(FEAT_SHIPPING_FREE),
        "shipping_speed_days" => Some(FEAT_SHIPPING_SPEED),
        "seller_reputation" => Some(FEAT_SELLER_REPUTATION),
        "variant_count" => Some(FEAT_VARIANT_COUNT),
        "deal_score" => Some(FEAT_DEAL_SCORE),
        "image_count" => Some(FEAT_IMAGE_COUNT),
        _ => None,
    }
}

/// Format a feature value as a human-readable example string.
fn format_feature_value(dim: usize, value: f32) -> String {
    match dim {
        FEAT_PRICE | FEAT_PRICE_ORIGINAL => format!("{value:.2}"),
        FEAT_DISCOUNT_PCT => format!("{:.0}%", value * 100.0),
        FEAT_RATING => format!("{value:.1}"),
        FEAT_REVIEW_COUNT_LOG => format!("{}", (10.0f32.powf(value)) as u64),
        FEAT_AVAILABILITY => {
            if value > 0.5 {
                "in_stock".to_string()
            } else {
                "out_of_stock".to_string()
            }
        }
        _ => format!("{value:.2}"),
    }
}

/// Add standard Schema.org fields for known types when structured data is present.
fn add_schema_org_fields(
    schema_type: &str,
    fields: &mut BTreeMap<String, Vec<(FieldType, f32, String)>>,
) {
    let schema_fields: &[(&str, FieldType)] = match schema_type {
        "Product" => &[
            ("brand", FieldType::String),
            ("category", FieldType::String),
            ("sku", FieldType::String),
            ("image_url", FieldType::Url),
            ("description", FieldType::String),
            ("currency", FieldType::String),
        ],
        "Article" => &[
            ("author", FieldType::String),
            ("published_date", FieldType::DateTime),
            ("category", FieldType::String),
            ("image_url", FieldType::Url),
            ("description", FieldType::String),
        ],
        "Organization" => &[
            ("description", FieldType::String),
            ("logo_url", FieldType::Url),
            ("address", FieldType::String),
            ("phone", FieldType::String),
            ("email", FieldType::String),
        ],
        "Event" => &[
            ("start_date", FieldType::DateTime),
            ("end_date", FieldType::DateTime),
            ("location", FieldType::String),
            ("organizer", FieldType::String),
            ("description", FieldType::String),
        ],
        "Review" => &[
            ("author", FieldType::String),
            ("body", FieldType::String),
            ("date_published", FieldType::DateTime),
        ],
        "Offer" => &[
            ("description", FieldType::String),
            ("valid_from", FieldType::DateTime),
            ("valid_through", FieldType::DateTime),
        ],
        _ => &[],
    };

    for (name, ftype) in schema_fields {
        fields.entry(name.to_string()).or_default().push((
            ftype.clone(),
            FieldSource::JsonLd.default_confidence(),
            String::new(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::builder::SiteMapBuilder;

    fn build_test_sitemap() -> SiteMap {
        let mut builder = SiteMapBuilder::new("shop.example.com");

        // Home page
        let mut home_feats = [0.0f32; FEATURE_DIM];
        home_feats[FEAT_HAS_STRUCTURED_DATA] = 1.0;
        home_feats[FEAT_PAGE_TYPE_CONFIDENCE] = 0.9;
        builder.add_node("https://shop.example.com/", PageType::Home, home_feats, 240);

        // Product listing
        let mut listing_feats = [0.0f32; FEATURE_DIM];
        listing_feats[FEAT_HAS_STRUCTURED_DATA] = 0.5;
        builder.add_node(
            "https://shop.example.com/category/electronics",
            PageType::ProductListing,
            listing_feats,
            200,
        );

        // Several products
        for i in 0..10 {
            let mut feats = [0.0f32; FEATURE_DIM];
            feats[FEAT_PRICE] = 100.0 + (i as f32 * 25.0);
            feats[FEAT_PRICE_ORIGINAL] = 150.0 + (i as f32 * 25.0);
            feats[FEAT_DISCOUNT_PCT] = 0.3;
            feats[FEAT_AVAILABILITY] = if i % 3 == 0 { 0.0 } else { 1.0 };
            feats[FEAT_RATING] = 3.5 + (i as f32 * 0.15);
            feats[FEAT_REVIEW_COUNT_LOG] = 2.0 + (i as f32 * 0.1);
            feats[FEAT_HAS_STRUCTURED_DATA] = 1.0;
            feats[FEAT_SELLER_REPUTATION] = 0.8;
            feats[FEAT_VARIANT_COUNT] = (2 + i % 5) as f32;
            feats[FEAT_IMAGE_COUNT] = (3 + i % 4) as f32;

            builder.add_node(
                &format!("https://shop.example.com/product/{i}"),
                PageType::ProductDetail,
                feats,
                220,
            );

            // Edges: listing → product
            builder.add_edge(
                1, // listing
                2 + i as u32,
                EdgeType::ContentLink,
                1,
                EdgeFlags::default(),
            );
        }

        // Articles
        for i in 0..5 {
            let mut feats = [0.0f32; FEATURE_DIM];
            feats[FEAT_TEXT_LENGTH_LOG] = 3.0 + i as f32 * 0.2;
            feats[FEAT_READING_LEVEL] = 0.6;
            feats[FEAT_RATING] = 4.0;
            feats[FEAT_HAS_STRUCTURED_DATA] = 0.8;
            feats[FEAT_IMAGE_COUNT] = 2.0;

            builder.add_node(
                &format!("https://shop.example.com/blog/{i}"),
                PageType::Article,
                feats,
                180,
            );
        }

        // Cart page
        let cart_feats = [0.0f32; FEATURE_DIM];
        builder.add_node(
            "https://shop.example.com/cart",
            PageType::Cart,
            cart_feats,
            200,
        );

        // Add some inter-type edges (product → product "related")
        for i in 2..8 {
            builder.add_edge(i, i + 1, EdgeType::Related, 2, EdgeFlags::default());
        }

        // Add actions on product pages
        for i in 2..12 {
            builder.add_action(i, OpCode::new(0x02, 0x00), -1, 0, 1); // add_to_cart
        }

        builder.build()
    }

    #[test]
    fn test_infer_schema_discovers_models() {
        let map = build_test_sitemap();
        let schema = infer_schema(&map, "shop.example.com");

        assert_eq!(schema.domain, "shop.example.com");
        assert!(
            schema.stats.total_models >= 2,
            "should find Product and Article at minimum"
        );

        // Find the Product model
        let product = schema.models.iter().find(|m| m.name == "Product");
        assert!(product.is_some(), "should discover Product model");

        let product = product.unwrap();
        assert_eq!(product.instance_count, 10);
        assert!(
            product.fields.len() >= 5,
            "Product should have several fields"
        );

        // Check specific fields exist
        let field_names: Vec<&str> = product.fields.iter().map(|f| f.name.as_str()).collect();
        assert!(field_names.contains(&"price"), "Product should have price");
        assert!(
            field_names.contains(&"rating"),
            "Product should have rating"
        );
        assert!(field_names.contains(&"url"), "Product should have url");
        assert!(
            field_names.contains(&"node_id"),
            "Product should have node_id"
        );
    }

    #[test]
    fn test_infer_schema_handles_empty_map() {
        let builder = SiteMapBuilder::new("empty.com");
        let map = builder.build();
        let schema = infer_schema(&map, "empty.com");

        assert_eq!(schema.stats.total_models, 0);
        assert!(schema.models.is_empty());
    }

    #[test]
    fn test_schema_field_nullability() {
        let map = build_test_sitemap();
        let schema = infer_schema(&map, "shop.example.com");

        let product = schema.models.iter().find(|m| m.name == "Product").unwrap();

        // url and node_id are not nullable
        let url_field = product.fields.iter().find(|f| f.name == "url").unwrap();
        assert!(!url_field.nullable);
    }

    #[test]
    fn test_schema_stats() {
        let map = build_test_sitemap();
        let schema = infer_schema(&map, "shop.example.com");

        assert!(schema.stats.total_models > 0);
        assert!(schema.stats.total_fields > 0);
        assert!(schema.stats.total_instances > 0);
        assert!(schema.stats.avg_confidence > 0.0);
        assert!(schema.stats.avg_confidence <= 1.0);
    }

    #[test]
    fn test_page_type_to_schema_org_mapping() {
        assert_eq!(
            page_type_to_schema_org(PageType::ProductDetail),
            Some("Product")
        );
        assert_eq!(page_type_to_schema_org(PageType::Article), Some("Article"));
        assert_eq!(page_type_to_schema_org(PageType::Cart), Some("Cart"));
        assert_eq!(page_type_to_schema_org(PageType::Unknown), None);
        assert_eq!(page_type_to_schema_org(PageType::ErrorPage), None);
    }

    #[test]
    fn test_simplify_model_name() {
        assert_eq!(simplify_model_name("FAQPage"), "FAQ");
        assert_eq!(simplify_model_name("Product"), "Product");
        assert_eq!(simplify_model_name("TechArticle"), "Article");
        assert_eq!(simplify_model_name("WebSite"), "Site");
    }

    // ── v4 Test Suite: Phase 1A — Schema Inference ──

    /// Build a realistic e-commerce sitemap simulating amazon.com-like structure.
    fn build_ecommerce_sitemap(domain: &str, product_count: usize) -> SiteMap {
        let mut builder = SiteMapBuilder::new(domain);

        // Home
        let mut hf = [0.0f32; FEATURE_DIM];
        hf[FEAT_HAS_STRUCTURED_DATA] = 1.0;
        hf[FEAT_SEARCH_AVAILABLE] = 1.0;
        builder.add_node(&format!("https://{domain}/"), PageType::Home, hf, 250);

        // Search results
        let mut sf = [0.0f32; FEATURE_DIM];
        sf[FEAT_PAGINATION_PRESENT] = 1.0;
        builder.add_node(
            &format!("https://{domain}/search"),
            PageType::SearchResults,
            sf,
            200,
        );

        // Category listing pages
        for i in 0..3 {
            let mut cf = [0.0f32; FEATURE_DIM];
            cf[FEAT_PAGINATION_PRESENT] = 1.0;
            cf[FEAT_FILTER_COUNT] = 5.0;
            cf[FEAT_HAS_STRUCTURED_DATA] = 0.8;
            builder.add_node(
                &format!("https://{domain}/category/{i}"),
                PageType::ProductListing,
                cf,
                200,
            );
        }

        // Products
        let cat_base = 2; // first category node index
        for i in 0..product_count {
            let mut pf = [0.0f32; FEATURE_DIM];
            pf[FEAT_PRICE] = 10.0 + (i as f32 * 15.0);
            pf[FEAT_PRICE_ORIGINAL] = 15.0 + (i as f32 * 15.0);
            pf[FEAT_DISCOUNT_PCT] = 0.2;
            pf[FEAT_AVAILABILITY] = if i % 5 == 0 { 0.0 } else { 1.0 };
            pf[FEAT_RATING] = 2.5 + (i as f32 * 0.1).min(2.5);
            pf[FEAT_REVIEW_COUNT_LOG] = 1.0 + (i as f32 * 0.05);
            pf[FEAT_HAS_STRUCTURED_DATA] = 1.0;
            pf[FEAT_SELLER_REPUTATION] = 0.75;
            pf[FEAT_VARIANT_COUNT] = (1 + i % 6) as f32;
            pf[FEAT_IMAGE_COUNT] = (2 + i % 5) as f32;
            pf[FEAT_DEAL_SCORE] = (i as f32 * 0.05).min(1.0);
            pf[FEAT_CATEGORY_PRICE_PERCENTILE] = i as f32 / product_count as f32;

            let node = builder.add_node(
                &format!("https://{domain}/product/{i}"),
                PageType::ProductDetail,
                pf,
                210,
            );

            // Category → product edge
            let cat = cat_base + (i % 3) as u32;
            builder.add_edge(cat, node, EdgeType::ContentLink, 1, EdgeFlags::default());

            // Add cart action
            builder.add_action(node, OpCode::new(0x02, 0x00), -1, 0, 1);
        }

        // Cart
        let cart_feats = [0.0f32; FEATURE_DIM];
        builder.add_node(
            &format!("https://{domain}/cart"),
            PageType::Cart,
            cart_feats,
            200,
        );

        // Login
        let login_feats = [0.0f32; FEATURE_DIM];
        builder.add_node(
            &format!("https://{domain}/login"),
            PageType::Login,
            login_feats,
            200,
        );

        builder.build()
    }

    /// Build a news/article sitemap simulating bbc.com-like structure.
    fn build_news_sitemap(domain: &str, article_count: usize) -> SiteMap {
        let mut builder = SiteMapBuilder::new(domain);

        let mut hf = [0.0f32; FEATURE_DIM];
        hf[FEAT_HAS_STRUCTURED_DATA] = 1.0;
        builder.add_node(&format!("https://{domain}/"), PageType::Home, hf, 250);

        for i in 0..article_count {
            let mut af = [0.0f32; FEATURE_DIM];
            af[FEAT_TEXT_LENGTH_LOG] = 3.0 + (i as f32 * 0.1);
            af[FEAT_READING_LEVEL] = 0.5 + (i as f32 * 0.02);
            af[FEAT_SENTIMENT] = 0.3 + (i as f32 * 0.05);
            af[FEAT_HAS_STRUCTURED_DATA] = 1.0;
            af[FEAT_IMAGE_COUNT] = (1 + i % 5) as f32;

            builder.add_node(
                &format!("https://{domain}/article/{i}"),
                PageType::Article,
                af,
                200,
            );
        }

        builder.build()
    }

    #[test]
    fn test_v4_schema_inference_ecommerce() {
        let map = build_ecommerce_sitemap("amazon.example.com", 20);
        let schema = infer_schema(&map, "amazon.example.com");

        // Must find Product model
        let product = schema.models.iter().find(|m| m.name == "Product");
        assert!(product.is_some(), "Must discover Product model");
        let product = product.unwrap();
        assert_eq!(product.instance_count, 20);

        // Product must have key fields
        let field_names: Vec<&str> = product.fields.iter().map(|f| f.name.as_str()).collect();
        assert!(field_names.contains(&"price"), "Product needs price field");
        assert!(
            field_names.contains(&"rating"),
            "Product needs rating field"
        );
        assert!(
            field_names.contains(&"availability"),
            "Product needs availability"
        );

        // Price field should be Float type with feature_dim
        let price = product.fields.iter().find(|f| f.name == "price").unwrap();
        assert_eq!(price.field_type, FieldType::Float);
        assert_eq!(price.feature_dim, Some(FEAT_PRICE));

        // Must also find Site/Home model
        let site = schema
            .models
            .iter()
            .find(|m| m.schema_org_type == "WebSite");
        assert!(
            site.is_some(),
            "Should discover WebSite model from Home page"
        );
    }

    #[test]
    fn test_v4_schema_inference_news() {
        let map = build_news_sitemap("bbc.example.com", 15);
        let schema = infer_schema(&map, "bbc.example.com");

        let article = schema.models.iter().find(|m| m.name == "Article");
        assert!(article.is_some(), "Must discover Article model");
        let article = article.unwrap();
        assert_eq!(article.instance_count, 15);

        let field_names: Vec<&str> = article.fields.iter().map(|f| f.name.as_str()).collect();
        assert!(
            field_names.contains(&"word_count"),
            "Article needs word_count"
        );
        assert!(
            field_names.contains(&"reading_level"),
            "Article needs reading_level"
        );
    }

    #[test]
    fn test_v4_schema_inference_multi_site() {
        // Test that schema inference works across diverse site types
        let sites: Vec<(&str, PageType, usize)> = vec![
            ("recipes.example.com", PageType::Article, 10),
            ("events.example.com", PageType::Calendar, 8),
            ("docs.example.com", PageType::Documentation, 12),
        ];

        for (domain, page_type, count) in &sites {
            let mut builder = SiteMapBuilder::new(domain);
            let hf = [0.0f32; FEATURE_DIM];
            builder.add_node(&format!("https://{domain}/"), PageType::Home, hf, 250);

            for i in 0..*count {
                let mut feats = [0.0f32; FEATURE_DIM];
                feats[FEAT_HAS_STRUCTURED_DATA] = 0.9;
                feats[FEAT_TEXT_LENGTH_LOG] = 2.0 + i as f32 * 0.1;
                builder.add_node(
                    &format!("https://{domain}/item/{i}"),
                    *page_type,
                    feats,
                    200,
                );
            }

            let map = builder.build();
            let schema = infer_schema(&map, domain);
            assert!(
                schema.stats.total_models >= 1,
                "{domain} should have at least 1 model"
            );
        }
    }

    #[test]
    fn test_v4_schema_field_types_correct() {
        let map = build_ecommerce_sitemap("typed.example.com", 10);
        let schema = infer_schema(&map, "typed.example.com");

        let product = schema.models.iter().find(|m| m.name == "Product").unwrap();

        // Verify field type correctness
        for field in &product.fields {
            match field.name.as_str() {
                "price"
                | "original_price"
                | "discount"
                | "rating"
                | "deal_score"
                | "category_price_percentile" => {
                    assert_eq!(
                        field.field_type,
                        FieldType::Float,
                        "{} should be Float",
                        field.name
                    );
                }
                "image_count" | "review_count" | "variant_count" => {
                    assert_eq!(
                        field.field_type,
                        FieldType::Integer,
                        "{} should be Integer",
                        field.name
                    );
                }
                "url" => {
                    assert_eq!(field.field_type, FieldType::Url, "url should be Url");
                }
                _ => {} // other fields are fine
            }
        }
    }

    #[test]
    fn test_v4_schema_actions_discovered() {
        let map = build_ecommerce_sitemap("actions.example.com", 10);
        let schema = infer_schema(&map, "actions.example.com");

        assert!(
            !schema.actions.is_empty(),
            "Should discover actions from opcodes"
        );

        // Should have add_to_cart action
        let atc = schema.actions.iter().find(|a| a.name == "add_to_cart");
        assert!(atc.is_some(), "Should find add_to_cart action");
    }

    #[test]
    fn test_v4_schema_confidence_ranges() {
        let map = build_ecommerce_sitemap("conf.example.com", 10);
        let schema = infer_schema(&map, "conf.example.com");

        for model in &schema.models {
            for field in &model.fields {
                assert!(
                    field.confidence > 0.0 && field.confidence <= 1.0,
                    "Field {} confidence should be in (0,1], got {}",
                    field.name,
                    field.confidence
                );
            }
        }

        assert!(
            schema.stats.avg_confidence > 0.0 && schema.stats.avg_confidence <= 1.0,
            "Avg confidence should be in (0,1]"
        );
    }
}
