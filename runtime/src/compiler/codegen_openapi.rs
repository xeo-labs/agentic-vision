//! OpenAPI 3.0 spec generator.
//!
//! Generates a YAML OpenAPI specification from the compiled schema.

use crate::compiler::models::*;

/// Generate an OpenAPI 3.0 YAML specification.
pub fn generate_openapi(schema: &CompiledSchema) -> String {
    let mut out = String::new();
    let domain = &schema.domain;

    // Info section
    out.push_str(&format!(
        r#"openapi: 3.0.3
info:
  title: {domain} — Auto-Generated API
  description: API compiled by Cortex Web Compiler from {domain}'s public structured data
  version: "1.0"
  contact:
    name: Cortex Web Compiler
"#
    ));

    // Paths
    out.push_str("paths:\n");

    for model in &schema.models {
        if model.instance_count <= 1 {
            continue;
        }

        let path_name = to_url_path(&model.name);

        // GET /models — list/search
        out.push_str(&format!("  /{path_name}:\n"));
        out.push_str("    get:\n");
        out.push_str(&format!("      summary: Search {}s\n", model.name));
        out.push_str(&format!("      operationId: search{}\n", model.name));
        out.push_str("      parameters:\n");
        out.push_str("        - name: query\n");
        out.push_str("          in: query\n");
        out.push_str("          schema:\n");
        out.push_str("            type: string\n");
        out.push_str("        - name: limit\n");
        out.push_str("          in: query\n");
        out.push_str("          schema:\n");
        out.push_str("            type: integer\n");
        out.push_str("            default: 20\n");

        // Add filter params for numeric fields
        for field in &model.fields {
            if field.feature_dim.is_some() {
                match field.field_type {
                    FieldType::Float | FieldType::Integer => {
                        out.push_str(&format!("        - name: {}_min\n", field.name));
                        out.push_str("          in: query\n");
                        out.push_str("          schema:\n");
                        out.push_str(&format!(
                            "            type: {}\n",
                            if field.field_type == FieldType::Integer {
                                "integer"
                            } else {
                                "number"
                            }
                        ));
                        out.push_str(&format!("        - name: {}_max\n", field.name));
                        out.push_str("          in: query\n");
                        out.push_str("          schema:\n");
                        out.push_str(&format!(
                            "            type: {}\n",
                            if field.field_type == FieldType::Integer {
                                "integer"
                            } else {
                                "number"
                            }
                        ));
                    }
                    _ => {}
                }
            }
        }

        out.push_str("      responses:\n");
        out.push_str("        '200':\n");
        out.push_str("          description: Successful response\n");
        out.push_str("          content:\n");
        out.push_str("            application/json:\n");
        out.push_str("              schema:\n");
        out.push_str("                type: array\n");
        out.push_str("                items:\n");
        out.push_str(&format!(
            "                  $ref: '#/components/schemas/{}'\n",
            model.name
        ));

        // GET /models/{nodeId} — get by ID
        out.push_str(&format!("  /{path_name}/{{nodeId}}:\n"));
        out.push_str("    get:\n");
        out.push_str(&format!("      summary: Get {} by node ID\n", model.name));
        out.push_str(&format!("      operationId: get{}\n", model.name));
        out.push_str("      parameters:\n");
        out.push_str("        - name: nodeId\n");
        out.push_str("          in: path\n");
        out.push_str("          required: true\n");
        out.push_str("          schema:\n");
        out.push_str("            type: integer\n");
        out.push_str("      responses:\n");
        out.push_str("        '200':\n");
        out.push_str("          description: Successful response\n");
        out.push_str("          content:\n");
        out.push_str("            application/json:\n");
        out.push_str("              schema:\n");
        out.push_str(&format!(
            "                $ref: '#/components/schemas/{}'\n",
            model.name
        ));
    }

    // Action paths
    for action in &schema.actions {
        if action.is_instance_method {
            let model_path = to_url_path(&action.belongs_to);
            let action_path = action.name.replace('_', "-");
            out.push_str(&format!("  /{model_path}/{{nodeId}}/{action_path}:\n"));
            out.push_str(&format!("    {}:\n", action.http_method.to_lowercase()));
            out.push_str(&format!(
                "      summary: {} on {}\n",
                action.name.replace('_', " "),
                action.belongs_to
            ));
            out.push_str(&format!(
                "      operationId: {}{}\n",
                action.name, action.belongs_to
            ));
            out.push_str("      parameters:\n");
            out.push_str("        - name: nodeId\n");
            out.push_str("          in: path\n");
            out.push_str("          required: true\n");
            out.push_str("          schema:\n");
            out.push_str("            type: integer\n");

            if !action.params.is_empty() {
                let non_node_params: Vec<&ActionParam> = action
                    .params
                    .iter()
                    .filter(|p| p.name != "node_id")
                    .collect();
                if !non_node_params.is_empty() && action.http_method == "POST" {
                    out.push_str("      requestBody:\n");
                    out.push_str("        content:\n");
                    out.push_str("          application/json:\n");
                    out.push_str("            schema:\n");
                    out.push_str("              type: object\n");
                    out.push_str("              properties:\n");
                    for param in &non_node_params {
                        out.push_str(&format!("                {}:\n", param.name));
                        let type_str = match param.param_type {
                            FieldType::Integer => "integer",
                            FieldType::Float => "number",
                            FieldType::Bool => "boolean",
                            _ => "string",
                        };
                        out.push_str(&format!("                  type: {type_str}\n"));
                    }
                }
            }

            out.push_str("      responses:\n");
            out.push_str("        '200':\n");
            out.push_str("          description: Action completed\n");
        }
    }

    // Component schemas
    out.push_str("components:\n");
    out.push_str("  schemas:\n");

    for model in &schema.models {
        out.push_str(&format!("    {}:\n", model.name));
        out.push_str("      type: object\n");
        out.push_str("      properties:\n");

        for field in &model.fields {
            out.push_str(&format!("        {}:\n", field.name));
            match &field.field_type {
                FieldType::String | FieldType::Url => {
                    out.push_str("          type: string\n");
                }
                FieldType::Float => {
                    out.push_str("          type: number\n");
                }
                FieldType::Integer => {
                    out.push_str("          type: integer\n");
                }
                FieldType::Bool => {
                    out.push_str("          type: boolean\n");
                }
                FieldType::DateTime => {
                    out.push_str("          type: string\n");
                    out.push_str("          format: date-time\n");
                }
                FieldType::Enum(variants) => {
                    out.push_str("          type: string\n");
                    out.push_str("          enum:\n");
                    for v in variants {
                        out.push_str(&format!("            - {v}\n"));
                    }
                }
                FieldType::Array(inner) => {
                    out.push_str("          type: array\n");
                    out.push_str("          items:\n");
                    let inner_type = match inner.as_ref() {
                        FieldType::String | FieldType::Url => "string",
                        FieldType::Float => "number",
                        FieldType::Integer => "integer",
                        _ => "string",
                    };
                    out.push_str(&format!("            type: {inner_type}\n"));
                }
                FieldType::Object(name) => {
                    out.push_str(&format!("          $ref: '#/components/schemas/{name}'\n"));
                }
            }
        }

        // Required fields
        let required: Vec<&str> = model
            .fields
            .iter()
            .filter(|f| !f.nullable)
            .map(|f| f.name.as_str())
            .collect();
        if !required.is_empty() {
            out.push_str("      required:\n");
            for r in &required {
                out.push_str(&format!("        - {r}\n"));
            }
        }
    }

    out
}

/// Convert a model name to a URL path segment.
fn to_url_path(name: &str) -> String {
    let mut result = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('-');
        }
        result.push(c.to_lowercase().next().unwrap_or(c));
    }
    result.push('s');
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn test_schema() -> CompiledSchema {
        CompiledSchema {
            domain: "test.com".to_string(),
            compiled_at: Utc::now(),
            models: vec![DataModel {
                name: "Product".to_string(),
                schema_org_type: "Product".to_string(),
                fields: vec![
                    ModelField {
                        name: "name".to_string(),
                        field_type: FieldType::String,
                        source: FieldSource::JsonLd,
                        confidence: 0.99,
                        nullable: false,
                        example_values: vec![],
                        feature_dim: None,
                    },
                    ModelField {
                        name: "price".to_string(),
                        field_type: FieldType::Float,
                        source: FieldSource::JsonLd,
                        confidence: 0.99,
                        nullable: true,
                        example_values: vec![],
                        feature_dim: Some(48),
                    },
                ],
                instance_count: 50,
                example_urls: vec![],
                search_action: None,
                list_url: None,
            }],
            actions: vec![],
            relationships: vec![],
            stats: SchemaStats {
                total_models: 1,
                total_fields: 2,
                total_instances: 50,
                avg_confidence: 0.99,
            },
        }
    }

    #[test]
    fn test_generate_openapi() {
        let spec = generate_openapi(&test_schema());
        assert!(spec.contains("openapi: 3.0.3"));
        assert!(spec.contains("test.com"));
        assert!(spec.contains("/products:"));
        assert!(spec.contains("Product:"));
        assert!(spec.contains("type: number"));
    }

    #[test]
    fn test_to_url_path() {
        assert_eq!(to_url_path("Product"), "products");
        assert_eq!(to_url_path("ForumPost"), "forum-posts");
        assert_eq!(to_url_path("FAQ"), "f-a-qs");
    }
}
