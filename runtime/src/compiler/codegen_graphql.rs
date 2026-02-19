//! GraphQL schema generator.
//!
//! Generates a GraphQL schema with types, queries, and mutations from compiled models.

use crate::compiler::models::*;

/// Generate a complete GraphQL schema from a compiled schema.
pub fn generate_graphql(schema: &CompiledSchema) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "# Auto-generated Cortex GraphQL schema for {}\n",
        schema.domain
    ));
    out.push_str(&format!(
        "# {} models, {} actions, {} relationships\n\n",
        schema.models.len(),
        schema.actions.len(),
        schema.relationships.len()
    ));

    // Generate types
    for model in &schema.models {
        generate_graphql_type(&mut out, model, schema);
    }

    // Generate Query type
    out.push_str("type Query {\n");
    for model in &schema.models {
        if model.instance_count <= 1 {
            // Singleton type — return single object
            let name_lower = model.name.to_lowercase();
            out.push_str(&format!("  {name_lower}: {name}\n", name = model.name));
        } else {
            // Collection type — search and get by ID
            let name_lower = pluralize_lower(&model.name);
            out.push_str(&format!(
                "  {name_lower}(query: String, limit: Int = 20): [{name}!]!\n",
                name = model.name
            ));
            out.push_str(&format!(
                "  {single}(nodeId: Int!): {name}\n",
                single = model.name.to_lowercase(),
                name = model.name
            ));
        }
    }
    out.push_str("}\n\n");

    // Generate Mutation type
    let mutations: Vec<&CompiledAction> = schema
        .actions
        .iter()
        .filter(|a| a.http_method == "POST")
        .collect();

    if !mutations.is_empty() {
        out.push_str("type Mutation {\n");
        for action in &mutations {
            let fn_name = to_camel_case(&action.name);
            let mut params: Vec<String> = Vec::new();

            if action.is_instance_method {
                params.push("nodeId: Int!".to_string());
            }

            for param in &action.params {
                if param.name == "node_id" {
                    continue;
                }
                let gql_type = param.param_type.to_graphql_type();
                let required = if param.required { "!" } else { "" };
                let default = if let Some(ref d) = param.default_value {
                    format!(" = {}", graphql_default(d, &param.param_type))
                } else {
                    String::new()
                };
                params.push(format!(
                    "{}: {gql_type}{required}{default}",
                    to_camel_case(&param.name)
                ));
            }

            let params_str = if params.is_empty() {
                String::new()
            } else {
                format!("({})", params.join(", "))
            };

            out.push_str(&format!("  {fn_name}{params_str}: Boolean!\n"));
        }
        out.push_str("}\n\n");
    }

    out
}

/// Generate a GraphQL type definition for a model.
fn generate_graphql_type(out: &mut String, model: &DataModel, schema: &CompiledSchema) {
    out.push_str(&format!("type {} {{\n", model.name));

    for field in &model.fields {
        let gql_type = field.field_type.to_graphql_type();
        let required = if !field.nullable { "!" } else { "" };
        out.push_str(&format!(
            "  {}: {gql_type}{required}\n",
            to_camel_case(&field.name)
        ));
    }

    // Relationship fields
    for rel in &schema.relationships {
        if rel.from_model == model.name {
            match rel.cardinality {
                Cardinality::BelongsTo | Cardinality::HasOne => {
                    out.push_str(&format!(
                        "  {}: {}\n",
                        to_camel_case(&rel.name),
                        rel.to_model
                    ));
                }
                Cardinality::HasMany | Cardinality::ManyToMany => {
                    out.push_str(&format!(
                        "  {}(limit: Int = 10): [{}!]!\n",
                        to_camel_case(&rel.name),
                        rel.to_model
                    ));
                }
            }
        }
    }

    out.push_str("}\n\n");
}

/// Convert snake_case to camelCase.
fn to_camel_case(s: &str) -> String {
    let parts: Vec<&str> = s.split('_').collect();
    if parts.is_empty() {
        return s.to_string();
    }
    let mut result = parts[0].to_string();
    for part in &parts[1..] {
        if let Some(first) = part.chars().next() {
            result.push(first.to_uppercase().next().unwrap_or(first));
            result.push_str(&part[first.len_utf8()..]);
        }
    }
    result
}

/// Generate a lowercase plural form for queries.
fn pluralize_lower(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower.ends_with('s') {
        format!("{lower}es")
    } else if lower.ends_with('y') {
        format!("{}ies", &lower[..lower.len() - 1])
    } else {
        format!("{lower}s")
    }
}

/// Convert a default value to GraphQL literal.
fn graphql_default(value: &str, field_type: &FieldType) -> String {
    match field_type {
        FieldType::Integer | FieldType::Float | FieldType::Bool => value.to_string(),
        FieldType::String | FieldType::Url => format!("\"{value}\""),
        _ => format!("\"{value}\""),
    }
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
                        name: "url".to_string(),
                        field_type: FieldType::Url,
                        source: FieldSource::Inferred,
                        confidence: 1.0,
                        nullable: false,
                        example_values: vec![],
                        feature_dim: None,
                    },
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
                total_fields: 3,
                total_instances: 50,
                avg_confidence: 0.99,
            },
        }
    }

    #[test]
    fn test_generate_graphql_has_type() {
        let gql = generate_graphql(&test_schema());
        assert!(gql.contains("type Product {"));
        assert!(gql.contains("url: String!"));
        assert!(gql.contains("price: Float"));
    }

    #[test]
    fn test_generate_graphql_has_query() {
        let gql = generate_graphql(&test_schema());
        assert!(gql.contains("type Query {"));
        assert!(gql.contains("products(query: String"));
    }

    #[test]
    fn test_generate_graphql_has_mutation() {
        let gql = generate_graphql(&test_schema());
        assert!(gql.contains("type Mutation {"));
        assert!(gql.contains("addToCart("));
    }

    #[test]
    fn test_pluralize_lower() {
        assert_eq!(pluralize_lower("Product"), "products");
        assert_eq!(pluralize_lower("Category"), "categories");
        assert_eq!(pluralize_lower("Address"), "addresses");
    }
}
