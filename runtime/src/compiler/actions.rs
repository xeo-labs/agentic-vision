//! Action compilation — transforms discovered HTTP actions into typed method definitions.
//!
//! Maps SiteMap action records to model methods (instance and class methods),
//! infers parameter types, and classifies actions by the model they belong to.

use crate::compiler::models::*;
use crate::map::types::*;
use std::collections::HashMap;

/// OpCode category constants (from the spec).
const OPCODE_NAV: u8 = 0x00;
const OPCODE_FORM: u8 = 0x01;
const OPCODE_CART: u8 = 0x02;
const OPCODE_AUTH: u8 = 0x03;
const OPCODE_MEDIA: u8 = 0x04;
const OPCODE_SOCIAL: u8 = 0x05;
const OPCODE_DATA: u8 = 0x06;

/// Compile actions from SiteMap action records into typed method definitions.
///
/// Groups actions by model, classifies as instance or class methods,
/// and infers parameter types.
pub fn compile_actions(site_map: &SiteMap, models: &[DataModel]) -> Vec<CompiledAction> {
    let mut compiled: Vec<CompiledAction> = Vec::new();
    let mut seen: HashMap<(String, String), bool> = HashMap::new();

    // Build node→model lookup
    let node_to_model = build_node_model_map(site_map, models);

    // Iterate over all actions in the site map
    for (node_idx, node) in site_map.nodes.iter().enumerate() {
        // Get actions for this node using CSR index
        let action_start = if node_idx < site_map.action_index.len() {
            site_map.action_index[node_idx] as usize
        } else {
            continue;
        };
        let action_end = if node_idx + 1 < site_map.action_index.len() {
            site_map.action_index[node_idx + 1] as usize
        } else {
            site_map.actions.len()
        };

        for action_idx in action_start..action_end {
            if action_idx >= site_map.actions.len() {
                break;
            }

            let action = &site_map.actions[action_idx];
            let (action_name, belongs_to, is_instance) =
                classify_action(action, node.page_type, &node_to_model, node_idx);

            // Deduplicate: only one definition per (model, action_name)
            let key = (belongs_to.clone(), action_name.clone());
            if seen.contains_key(&key) {
                continue;
            }
            seen.insert(key, true);

            let (http_method, endpoint, params) =
                infer_action_details(action, &action_name, &belongs_to);

            let execution_path = if action.http_executable {
                "http".to_string()
            } else {
                "browser".to_string()
            };

            compiled.push(CompiledAction {
                name: action_name,
                belongs_to,
                is_instance_method: is_instance,
                http_method,
                endpoint_template: endpoint,
                params,
                requires_auth: action.risk >= 1 || action.opcode.category == OPCODE_AUTH,
                execution_path,
                confidence: if action.http_executable { 0.9 } else { 0.7 },
            });
        }
    }

    // Add well-known global actions
    add_global_actions(&mut compiled, site_map, &seen);

    compiled
}

/// Build a lookup from node index to model name.
fn build_node_model_map(site_map: &SiteMap, models: &[DataModel]) -> HashMap<usize, String> {
    let mut map: HashMap<usize, String> = HashMap::new();

    for model in models {
        for (idx, node) in site_map.nodes.iter().enumerate() {
            if let Some(schema_type) = page_type_to_schema_name(node.page_type) {
                if schema_type == model.schema_org_type {
                    map.insert(idx, model.name.clone());
                }
            }
        }
    }

    map
}

/// Map PageType to Schema.org type name.
fn page_type_to_schema_name(pt: PageType) -> Option<&'static str> {
    match pt {
        PageType::ProductDetail => Some("Product"),
        PageType::ProductListing => Some("ProductListing"),
        PageType::Article => Some("Article"),
        PageType::ReviewList => Some("Review"),
        PageType::Cart => Some("Cart"),
        PageType::Checkout => Some("CheckoutPage"),
        PageType::Account => Some("Account"),
        PageType::Login => Some("LoginPage"),
        PageType::Home => Some("WebSite"),
        PageType::SearchResults => Some("SearchResultsPage"),
        _ => None,
    }
}

/// Classify an action: determine its name, which model it belongs to, and whether
/// it's an instance method.
fn classify_action(
    action: &ActionRecord,
    _page_type: PageType,
    node_to_model: &HashMap<usize, String>,
    node_idx: usize,
) -> (String, String, bool) {
    let cat = action.opcode.category;
    let act = action.opcode.action;

    let (name, model, is_instance) = match cat {
        OPCODE_NAV => match act {
            0x00 => ("click".to_string(), "Site".to_string(), false),
            0x01 => ("navigate".to_string(), "Site".to_string(), false),
            0x02 => ("scroll".to_string(), "Site".to_string(), false),
            _ => ("navigate".to_string(), "Site".to_string(), false),
        },
        OPCODE_FORM => match act {
            0x00 => ("submit_form".to_string(), "Site".to_string(), false),
            0x01 => ("search".to_string(), "Site".to_string(), false),
            0x02 => ("filter".to_string(), "Site".to_string(), false),
            0x03 => ("sort".to_string(), "Site".to_string(), false),
            _ => ("submit".to_string(), "Site".to_string(), false),
        },
        OPCODE_CART => {
            let model = node_to_model
                .get(&node_idx)
                .cloned()
                .unwrap_or_else(|| "Product".to_string());
            match act {
                0x00 => ("add_to_cart".to_string(), model, true),
                0x01 => ("remove_from_cart".to_string(), "Cart".to_string(), true),
                0x02 => ("update_quantity".to_string(), "Cart".to_string(), true),
                0x03 => ("apply_coupon".to_string(), "Cart".to_string(), false),
                0x04 => ("checkout".to_string(), "Cart".to_string(), false),
                0x05 => ("add_to_wishlist".to_string(), model, true),
                _ => ("cart_action".to_string(), "Cart".to_string(), false),
            }
        }
        OPCODE_AUTH => match act {
            0x00 => ("login".to_string(), "Site".to_string(), false),
            0x01 => ("logout".to_string(), "Site".to_string(), false),
            0x02 => ("register".to_string(), "Site".to_string(), false),
            _ => ("auth_action".to_string(), "Site".to_string(), false),
        },
        OPCODE_MEDIA => match act {
            0x00 => ("play".to_string(), "Media".to_string(), true),
            0x01 => ("pause".to_string(), "Media".to_string(), true),
            0x02 => ("download".to_string(), "Media".to_string(), true),
            _ => ("media_action".to_string(), "Media".to_string(), true),
        },
        OPCODE_SOCIAL => match act {
            0x00 => ("like".to_string(), "Site".to_string(), true),
            0x01 => ("share".to_string(), "Site".to_string(), true),
            0x02 => ("comment".to_string(), "Site".to_string(), true),
            0x03 => ("follow".to_string(), "Site".to_string(), true),
            _ => ("social_action".to_string(), "Site".to_string(), true),
        },
        OPCODE_DATA => match act {
            0x00 => ("export".to_string(), "Site".to_string(), false),
            0x01 => ("import".to_string(), "Site".to_string(), false),
            _ => ("data_action".to_string(), "Site".to_string(), false),
        },
        _ => {
            let model = node_to_model
                .get(&node_idx)
                .cloned()
                .unwrap_or_else(|| "Site".to_string());
            (format!("action_{cat:02x}_{act:02x}"), model, false)
        }
    };

    (name, model, is_instance)
}

/// Infer HTTP details for a compiled action.
fn infer_action_details(
    action: &ActionRecord,
    name: &str,
    _belongs_to: &str,
) -> (String, String, Vec<ActionParam>) {
    let cat = action.opcode.category;

    match name {
        "search" => (
            "GET".to_string(),
            "/search?q={query}".to_string(),
            vec![ActionParam {
                name: "query".to_string(),
                param_type: FieldType::String,
                required: true,
                default_value: None,
                source: "url_param".to_string(),
            }],
        ),
        "add_to_cart" => (
            "POST".to_string(),
            "/cart/add".to_string(),
            vec![
                ActionParam {
                    name: "node_id".to_string(),
                    param_type: FieldType::Integer,
                    required: true,
                    default_value: None,
                    source: "json_body".to_string(),
                },
                ActionParam {
                    name: "quantity".to_string(),
                    param_type: FieldType::Integer,
                    required: false,
                    default_value: Some("1".to_string()),
                    source: "json_body".to_string(),
                },
            ],
        ),
        "remove_from_cart" => (
            "POST".to_string(),
            "/cart/remove".to_string(),
            vec![ActionParam {
                name: "node_id".to_string(),
                param_type: FieldType::Integer,
                required: true,
                default_value: None,
                source: "json_body".to_string(),
            }],
        ),
        "apply_coupon" => (
            "POST".to_string(),
            "/cart/coupon".to_string(),
            vec![ActionParam {
                name: "code".to_string(),
                param_type: FieldType::String,
                required: true,
                default_value: None,
                source: "json_body".to_string(),
            }],
        ),
        "checkout" => (
            "POST".to_string(),
            "/checkout".to_string(),
            vec![ActionParam {
                name: "payment_method".to_string(),
                param_type: FieldType::String,
                required: false,
                default_value: Some("saved_card".to_string()),
                source: "json_body".to_string(),
            }],
        ),
        "login" => (
            "POST".to_string(),
            "/auth/login".to_string(),
            vec![
                ActionParam {
                    name: "email".to_string(),
                    param_type: FieldType::String,
                    required: true,
                    default_value: None,
                    source: "json_body".to_string(),
                },
                ActionParam {
                    name: "password".to_string(),
                    param_type: FieldType::String,
                    required: true,
                    default_value: None,
                    source: "json_body".to_string(),
                },
            ],
        ),
        "filter" | "sort" => (
            "GET".to_string(),
            format!("/{name}"),
            vec![ActionParam {
                name: "criteria".to_string(),
                param_type: FieldType::String,
                required: true,
                default_value: None,
                source: "url_param".to_string(),
            }],
        ),
        _ => {
            // Default: POST with empty params
            let method = if cat == OPCODE_NAV || cat == OPCODE_FORM {
                "GET"
            } else {
                "POST"
            };
            (
                method.to_string(),
                format!("/{}", name.replace('_', "-")),
                Vec::new(),
            )
        }
    }
}

/// Add well-known global actions that most sites have.
fn add_global_actions(
    compiled: &mut Vec<CompiledAction>,
    site_map: &SiteMap,
    seen: &HashMap<(String, String), bool>,
) {
    // If site has search results pages but no search action, add one
    let has_search = site_map
        .nodes
        .iter()
        .any(|n| n.page_type == PageType::SearchResults);
    if has_search && !seen.contains_key(&("Site".to_string(), "search".to_string())) {
        compiled.push(CompiledAction {
            name: "search".to_string(),
            belongs_to: "Site".to_string(),
            is_instance_method: false,
            http_method: "GET".to_string(),
            endpoint_template: "/search?q={query}".to_string(),
            params: vec![ActionParam {
                name: "query".to_string(),
                param_type: FieldType::String,
                required: true,
                default_value: None,
                source: "url_param".to_string(),
            }],
            requires_auth: false,
            execution_path: "http".to_string(),
            confidence: 0.8,
        });
    }

    // If site has a cart page but no view_cart action, add one
    let has_cart = site_map.nodes.iter().any(|n| n.page_type == PageType::Cart);
    if has_cart && !seen.contains_key(&("Cart".to_string(), "view".to_string())) {
        compiled.push(CompiledAction {
            name: "view".to_string(),
            belongs_to: "Cart".to_string(),
            is_instance_method: false,
            http_method: "GET".to_string(),
            endpoint_template: "/cart".to_string(),
            params: Vec::new(),
            requires_auth: false,
            execution_path: "http".to_string(),
            confidence: 0.75,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::builder::SiteMapBuilder;

    #[test]
    fn test_compile_actions_basic() {
        let mut builder = SiteMapBuilder::new("shop.com");

        let feats = [0.0f32; FEATURE_DIM];
        builder.add_node("https://shop.com/", PageType::Home, feats, 240);
        builder.add_node(
            "https://shop.com/product/1",
            PageType::ProductDetail,
            feats,
            200,
        );
        builder.add_node("https://shop.com/cart", PageType::Cart, feats, 200);
        builder.add_node(
            "https://shop.com/search",
            PageType::SearchResults,
            feats,
            200,
        );

        // Add actions
        builder.add_action(1, OpCode::new(OPCODE_CART, 0x00), -1, 0, 1); // add_to_cart
        builder.add_action(2, OpCode::new(OPCODE_CART, 0x04), -1, 0, 1); // checkout

        let map = builder.build();
        let models = vec![
            DataModel {
                name: "Product".to_string(),
                schema_org_type: "Product".to_string(),
                fields: vec![],
                instance_count: 1,
                example_urls: vec![],
                search_action: None,
                list_url: None,
            },
            DataModel {
                name: "Cart".to_string(),
                schema_org_type: "Cart".to_string(),
                fields: vec![],
                instance_count: 1,
                example_urls: vec![],
                search_action: None,
                list_url: None,
            },
        ];

        let actions = compile_actions(&map, &models);
        assert!(!actions.is_empty());

        // Should find add_to_cart
        let atc = actions.iter().find(|a| a.name == "add_to_cart");
        assert!(atc.is_some(), "should find add_to_cart action");
        let atc = atc.unwrap();
        assert!(atc.is_instance_method);
        assert_eq!(atc.http_method, "POST");

        // Should find checkout
        let checkout = actions.iter().find(|a| a.name == "checkout");
        assert!(checkout.is_some(), "should find checkout action");
    }

    #[test]
    fn test_compile_actions_adds_global_search() {
        let mut builder = SiteMapBuilder::new("news.com");
        let feats = [0.0f32; FEATURE_DIM];

        builder.add_node("https://news.com/", PageType::Home, feats, 240);
        builder.add_node(
            "https://news.com/search",
            PageType::SearchResults,
            feats,
            200,
        );

        let map = builder.build();
        let actions = compile_actions(&map, &[]);

        let search = actions.iter().find(|a| a.name == "search");
        assert!(search.is_some(), "should auto-add search action");
        let search = search.unwrap();
        assert_eq!(search.belongs_to, "Site");
        assert!(!search.is_instance_method);
    }

    #[test]
    fn test_classify_action_cart_opcode() {
        let action = ActionRecord {
            opcode: OpCode::new(OPCODE_CART, 0x00),
            target_node: -1,
            cost_hint: 0,
            risk: 0,
            http_executable: true,
        };

        let node_to_model = HashMap::from([(5usize, "Product".to_string())]);

        let (name, model, is_instance) =
            classify_action(&action, PageType::ProductDetail, &node_to_model, 5);

        assert_eq!(name, "add_to_cart");
        assert_eq!(model, "Product");
        assert!(is_instance);
    }
}
