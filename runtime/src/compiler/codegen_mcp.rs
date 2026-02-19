//! MCP tool definition generator.
//!
//! Generates MCP tool definitions scoped to a specific compiled site.

use crate::compiler::models::*;

/// Generate MCP tool definitions JSON from a compiled schema.
pub fn generate_mcp(schema: &CompiledSchema) -> String {
    let domain = &schema.domain;
    let domain_prefix = domain.replace(['.', '-'], "_");

    let mut tools: Vec<serde_json::Value> = Vec::new();

    // Generate search tools for each collection model
    for model in &schema.models {
        if model.instance_count <= 1 {
            continue;
        }

        let model_lower = model.name.to_lowercase();

        // Search tool
        let mut search_props = serde_json::Map::new();
        search_props.insert(
            "query".to_string(),
            serde_json::json!({"type": "string", "description": format!("Search query for {}s", model.name)}),
        );
        search_props.insert(
            "limit".to_string(),
            serde_json::json!({"type": "integer", "description": "Max results", "default": 20}),
        );

        for field in &model.fields {
            if let Some(_dim) = field.feature_dim {
                match field.field_type {
                    FieldType::Float | FieldType::Integer => {
                        search_props.insert(
                            format!("{}_min", field.name),
                            serde_json::json!({
                                "type": "number",
                                "description": format!("Minimum {}", field.name)
                            }),
                        );
                        search_props.insert(
                            format!("{}_max", field.name),
                            serde_json::json!({
                                "type": "number",
                                "description": format!("Maximum {}", field.name)
                            }),
                        );
                    }
                    _ => {}
                }
            }
        }

        tools.push(serde_json::json!({
            "name": format!("{domain_prefix}_search_{model_lower}s"),
            "description": format!("Search {}s on {domain}", model.name),
            "inputSchema": {
                "type": "object",
                "properties": search_props,
                "required": ["query"]
            }
        }));

        // Get by ID tool
        tools.push(serde_json::json!({
            "name": format!("{domain_prefix}_get_{model_lower}"),
            "description": format!("Get a specific {} by node ID from {domain}", model.name),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "node_id": {
                        "type": "integer",
                        "description": format!("Node ID of the {}", model.name)
                    }
                },
                "required": ["node_id"]
            }
        }));
    }

    // Generate action tools
    for action in &schema.actions {
        let mut props = serde_json::Map::new();

        if action.is_instance_method {
            props.insert(
                "node_id".to_string(),
                serde_json::json!({
                    "type": "integer",
                    "description": format!("Node ID of the {}", action.belongs_to)
                }),
            );
        }

        let mut required: Vec<String> = Vec::new();
        if action.is_instance_method {
            required.push("node_id".to_string());
        }

        for param in &action.params {
            if param.name == "node_id" {
                continue;
            }
            let type_str = match param.param_type {
                FieldType::Integer => "integer",
                FieldType::Float => "number",
                FieldType::Bool => "boolean",
                _ => "string",
            };
            let mut prop = serde_json::json!({
                "type": type_str,
                "description": format!("{} parameter", param.name)
            });
            if let Some(ref default) = param.default_value {
                prop["default"] = serde_json::Value::String(default.clone());
            }
            props.insert(param.name.clone(), prop);
            if param.required {
                required.push(param.name.clone());
            }
        }

        tools.push(serde_json::json!({
            "name": format!("{domain_prefix}_{}", action.name),
            "description": format!("{} on {domain}", action.name.replace('_', " ")),
            "inputSchema": {
                "type": "object",
                "properties": props,
                "required": required
            }
        }));
    }

    let output = serde_json::json!({ "tools": tools });
    serde_json::to_string_pretty(&output).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn test_schema() -> CompiledSchema {
        CompiledSchema {
            domain: "shop.com".to_string(),
            compiled_at: Utc::now(),
            models: vec![DataModel {
                name: "Product".to_string(),
                schema_org_type: "Product".to_string(),
                fields: vec![ModelField {
                    name: "price".to_string(),
                    field_type: FieldType::Float,
                    source: FieldSource::JsonLd,
                    confidence: 0.99,
                    nullable: true,
                    example_values: vec![],
                    feature_dim: Some(48),
                }],
                instance_count: 50,
                example_urls: vec![],
                search_action: None,
                list_url: None,
            }],
            actions: vec![CompiledAction {
                name: "add_to_cart".to_string(),
                belongs_to: "Product".to_string(),
                is_instance_method: true,
                http_method: "POST".to_string(),
                endpoint_template: "/cart/add".to_string(),
                params: vec![ActionParam {
                    name: "quantity".to_string(),
                    param_type: FieldType::Integer,
                    required: false,
                    default_value: Some("1".to_string()),
                    source: "json_body".to_string(),
                }],
                requires_auth: false,
                execution_path: "http".to_string(),
                confidence: 0.9,
            }],
            relationships: vec![],
            stats: SchemaStats {
                total_models: 1,
                total_fields: 1,
                total_instances: 50,
                avg_confidence: 0.99,
            },
        }
    }

    #[test]
    fn test_generate_mcp_tools() {
        let json = generate_mcp(&test_schema());
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        let tools = parsed["tools"].as_array().unwrap();
        assert!(tools.len() >= 2, "should have search + action tools");

        // Should have search tool
        let search = tools
            .iter()
            .find(|t| t["name"].as_str().unwrap().contains("search"));
        assert!(search.is_some(), "should have search tool");

        // Should have add_to_cart tool
        let cart = tools
            .iter()
            .find(|t| t["name"].as_str().unwrap().contains("add_to_cart"));
        assert!(cart.is_some(), "should have add_to_cart tool");
    }

    #[test]
    fn test_mcp_valid_json() {
        let json = generate_mcp(&test_schema());
        let result: Result<serde_json::Value, _> = serde_json::from_str(&json);
        assert!(result.is_ok(), "output should be valid JSON");
    }
}
