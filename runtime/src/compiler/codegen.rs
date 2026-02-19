//! Top-level code generation orchestrator.
//!
//! Calls each sub-generator (Python, TypeScript, OpenAPI, GraphQL, MCP) and
//! assembles the complete set of generated files.

use crate::compiler::codegen_graphql;
use crate::compiler::codegen_mcp;
use crate::compiler::codegen_openapi;
use crate::compiler::codegen_python;
use crate::compiler::codegen_typescript;
use crate::compiler::models::*;
use anyhow::Result;
use std::fs;
use std::path::Path;

/// Generate all client files from a compiled schema.
///
/// Writes files to `output_dir` and returns metadata about what was generated.
pub fn generate_all(schema: &CompiledSchema, output_dir: &Path) -> Result<GeneratedFiles> {
    fs::create_dir_all(output_dir)?;

    let mut files: Vec<GeneratedFile> = Vec::new();

    // Python client
    let python = codegen_python::generate_python(schema);
    let python_path = output_dir.join("client.py");
    fs::write(&python_path, &python)?;
    files.push(GeneratedFile {
        filename: "client.py".to_string(),
        size: python.len(),
        content: python,
    });

    // TypeScript client
    let typescript = codegen_typescript::generate_typescript(schema);
    let ts_path = output_dir.join("client.ts");
    fs::write(&ts_path, &typescript)?;
    files.push(GeneratedFile {
        filename: "client.ts".to_string(),
        size: typescript.len(),
        content: typescript,
    });

    // OpenAPI spec
    let openapi = codegen_openapi::generate_openapi(schema);
    let openapi_path = output_dir.join("openapi.yaml");
    fs::write(&openapi_path, &openapi)?;
    files.push(GeneratedFile {
        filename: "openapi.yaml".to_string(),
        size: openapi.len(),
        content: openapi,
    });

    // GraphQL schema
    let graphql = codegen_graphql::generate_graphql(schema);
    let graphql_path = output_dir.join("schema.graphql");
    fs::write(&graphql_path, &graphql)?;
    files.push(GeneratedFile {
        filename: "schema.graphql".to_string(),
        size: graphql.len(),
        content: graphql,
    });

    // MCP tools
    let mcp = codegen_mcp::generate_mcp(schema);
    let mcp_path = output_dir.join("mcp_tools.json");
    fs::write(&mcp_path, &mcp)?;
    files.push(GeneratedFile {
        filename: "mcp_tools.json".to_string(),
        size: mcp.len(),
        content: mcp,
    });

    Ok(GeneratedFiles { files })
}

/// Generate all client files as in-memory strings (no disk write).
pub fn generate_all_in_memory(schema: &CompiledSchema) -> GeneratedFiles {
    let python = codegen_python::generate_python(schema);
    let typescript = codegen_typescript::generate_typescript(schema);
    let openapi = codegen_openapi::generate_openapi(schema);
    let graphql = codegen_graphql::generate_graphql(schema);
    let mcp = codegen_mcp::generate_mcp(schema);

    GeneratedFiles {
        files: vec![
            GeneratedFile {
                filename: "client.py".to_string(),
                size: python.len(),
                content: python,
            },
            GeneratedFile {
                filename: "client.ts".to_string(),
                size: typescript.len(),
                content: typescript,
            },
            GeneratedFile {
                filename: "openapi.yaml".to_string(),
                size: openapi.len(),
                content: openapi,
            },
            GeneratedFile {
                filename: "schema.graphql".to_string(),
                size: graphql.len(),
                content: graphql,
            },
            GeneratedFile {
                filename: "mcp_tools.json".to_string(),
                size: mcp.len(),
                content: mcp,
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::TempDir;

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
                        example_values: vec!["https://test.com/p/1".to_string()],
                        feature_dim: None,
                    },
                    ModelField {
                        name: "name".to_string(),
                        field_type: FieldType::String,
                        source: FieldSource::JsonLd,
                        confidence: 0.99,
                        nullable: false,
                        example_values: vec!["Widget".to_string()],
                        feature_dim: None,
                    },
                    ModelField {
                        name: "price".to_string(),
                        field_type: FieldType::Float,
                        source: FieldSource::JsonLd,
                        confidence: 0.99,
                        nullable: true,
                        example_values: vec!["29.99".to_string()],
                        feature_dim: Some(48),
                    },
                ],
                instance_count: 100,
                example_urls: vec!["https://test.com/p/1".to_string()],
                search_action: None,
                list_url: Some("https://test.com/products".to_string()),
            }],
            actions: vec![CompiledAction {
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
                confidence: 0.9,
            }],
            relationships: vec![],
            stats: SchemaStats {
                total_models: 1,
                total_fields: 3,
                total_instances: 100,
                avg_confidence: 0.99,
            },
        }
    }

    #[test]
    fn test_generate_all_creates_files() {
        let schema = test_schema();
        let dir = TempDir::new().unwrap();

        let result = generate_all(&schema, dir.path()).unwrap();
        assert_eq!(result.files.len(), 5);

        // Verify files exist on disk
        assert!(dir.path().join("client.py").exists());
        assert!(dir.path().join("client.ts").exists());
        assert!(dir.path().join("openapi.yaml").exists());
        assert!(dir.path().join("schema.graphql").exists());
        assert!(dir.path().join("mcp_tools.json").exists());
    }

    #[test]
    fn test_generate_all_in_memory() {
        let schema = test_schema();
        let result = generate_all_in_memory(&schema);
        assert_eq!(result.files.len(), 5);

        for file in &result.files {
            assert!(
                !file.content.is_empty(),
                "{} should not be empty",
                file.filename
            );
            assert!(file.size > 0);
        }
    }

    // ── v4 Test Suite: Phase 1B — Code Generation ──

    fn full_ecommerce_schema() -> CompiledSchema {
        CompiledSchema {
            domain: "shop.example.com".to_string(),
            compiled_at: Utc::now(),
            models: vec![
                DataModel {
                    name: "Product".to_string(),
                    schema_org_type: "Product".to_string(),
                    fields: vec![
                        ModelField {
                            name: "url".to_string(),
                            field_type: FieldType::Url,
                            source: FieldSource::Inferred,
                            confidence: 1.0,
                            nullable: false,
                            example_values: vec!["https://shop.example.com/p/1".to_string()],
                            feature_dim: None,
                        },
                        ModelField {
                            name: "name".to_string(),
                            field_type: FieldType::String,
                            source: FieldSource::JsonLd,
                            confidence: 0.99,
                            nullable: false,
                            example_values: vec!["Widget".to_string()],
                            feature_dim: None,
                        },
                        ModelField {
                            name: "price".to_string(),
                            field_type: FieldType::Float,
                            source: FieldSource::JsonLd,
                            confidence: 0.99,
                            nullable: true,
                            example_values: vec!["29.99".to_string()],
                            feature_dim: Some(48),
                        },
                        ModelField {
                            name: "rating".to_string(),
                            field_type: FieldType::Float,
                            source: FieldSource::JsonLd,
                            confidence: 0.95,
                            nullable: true,
                            example_values: vec!["4.5".to_string()],
                            feature_dim: Some(52),
                        },
                        ModelField {
                            name: "availability".to_string(),
                            field_type: FieldType::Bool,
                            source: FieldSource::Inferred,
                            confidence: 0.85,
                            nullable: true,
                            example_values: vec![],
                            feature_dim: Some(51),
                        },
                    ],
                    instance_count: 500,
                    example_urls: vec!["https://shop.example.com/p/1".to_string()],
                    search_action: Some(CompiledAction {
                        name: "search".to_string(),
                        belongs_to: "Product".to_string(),
                        is_instance_method: false,
                        http_method: "GET".to_string(),
                        endpoint_template: "/search?q={query}".to_string(),
                        params: vec![],
                        requires_auth: false,
                        execution_path: "http".to_string(),
                        confidence: 0.9,
                    }),
                    list_url: Some("https://shop.example.com/products".to_string()),
                },
                DataModel {
                    name: "Category".to_string(),
                    schema_org_type: "ProductListing".to_string(),
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
                            source: FieldSource::Inferred,
                            confidence: 0.8,
                            nullable: false,
                            example_values: vec![],
                            feature_dim: None,
                        },
                    ],
                    instance_count: 10,
                    example_urls: vec!["https://shop.example.com/electronics".to_string()],
                    search_action: None,
                    list_url: None,
                },
            ],
            actions: vec![
                CompiledAction {
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
                },
                CompiledAction {
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
                    confidence: 0.95,
                },
            ],
            relationships: vec![ModelRelationship {
                from_model: "Product".to_string(),
                to_model: "Category".to_string(),
                name: "belongs_to_category".to_string(),
                cardinality: Cardinality::BelongsTo,
                edge_count: 500,
                traversal_hint: TraversalHint {
                    edge_types: vec!["Breadcrumb".to_string()],
                    forward: true,
                },
            }],
            stats: SchemaStats {
                total_models: 2,
                total_fields: 7,
                total_instances: 510,
                avg_confidence: 0.93,
            },
        }
    }

    #[test]
    fn test_v4_codegen_python_valid_syntax() {
        let schema = full_ecommerce_schema();
        let files = generate_all_in_memory(&schema);

        let py_file = files
            .files
            .iter()
            .find(|f| f.filename == "client.py")
            .unwrap();
        let code = &py_file.content;

        // Must have imports
        assert!(code.contains("from __future__ import annotations"));
        assert!(code.contains("from dataclasses import dataclass"));

        // Must have Product class
        assert!(code.contains("@dataclass\nclass Product:"));
        assert!(code.contains("price: Optional[float]"));
        assert!(code.contains("rating: Optional[float]"));

        // Must have Category class
        assert!(code.contains("@dataclass\nclass Category:"));

        // Must have methods
        assert!(code.contains("def search("), "search method");
        assert!(code.contains("def add_to_cart(self"), "add_to_cart method");
        assert!(
            code.contains("def _from_node(node)"),
            "_from_node deserializer"
        );
        assert!(code.contains("def _field_to_dim("), "_field_to_dim helper");

        // Must have relationship method
        assert!(
            code.contains("belongs_to_category"),
            "relationship traversal"
        );

        // Should not have Python syntax errors (basic checks)
        assert!(
            !code.contains("None,\n    )"),
            "trailing comma is fine in Python"
        );
    }

    #[test]
    fn test_v4_codegen_typescript_valid() {
        let schema = full_ecommerce_schema();
        let files = generate_all_in_memory(&schema);

        let ts_file = files
            .files
            .iter()
            .find(|f| f.filename == "client.ts")
            .unwrap();
        let code = &ts_file.content;

        assert!(code.contains("interface Product"), "Product interface");
        assert!(code.contains("interface Category"), "Category interface");
        assert!(code.contains("price?:"), "optional price field");
        assert!(code.contains("async function"), "async functions");
    }

    #[test]
    fn test_v4_codegen_openapi_valid_yaml() {
        let schema = full_ecommerce_schema();
        let files = generate_all_in_memory(&schema);

        let openapi = files
            .files
            .iter()
            .find(|f| f.filename == "openapi.yaml")
            .unwrap();
        let code = &openapi.content;

        assert!(code.contains("openapi: 3.0.3"), "OpenAPI version");
        assert!(code.contains("paths:"), "paths section");
        assert!(code.contains("components:"), "components section");
        assert!(code.contains("/products"), "products path");
        assert!(code.contains("schemas:"), "schemas section");
        assert!(code.contains("Product:"), "Product schema");
    }

    #[test]
    fn test_v4_codegen_graphql_valid() {
        let schema = full_ecommerce_schema();
        let files = generate_all_in_memory(&schema);

        let gql = files
            .files
            .iter()
            .find(|f| f.filename == "schema.graphql")
            .unwrap();
        let code = &gql.content;

        assert!(code.contains("type Product"), "Product type");
        assert!(code.contains("type Category"), "Category type");
        assert!(code.contains("type Query"), "Query type");
    }

    #[test]
    fn test_v4_codegen_mcp_valid_json() {
        let schema = full_ecommerce_schema();
        let files = generate_all_in_memory(&schema);

        let mcp = files
            .files
            .iter()
            .find(|f| f.filename == "mcp_tools.json")
            .unwrap();

        // Parse as JSON to validate
        let parsed: serde_json::Value =
            serde_json::from_str(&mcp.content).expect("MCP tools file should be valid JSON");

        let tools = parsed.get("tools").expect("should have tools array");
        assert!(tools.is_array());
        assert!(
            tools.as_array().unwrap().len() >= 1,
            "should have at least 1 tool"
        );

        // Each tool should have name, description, inputSchema
        for tool in tools.as_array().unwrap() {
            assert!(tool.get("name").is_some(), "tool needs name");
            assert!(tool.get("description").is_some(), "tool needs description");
            assert!(tool.get("inputSchema").is_some(), "tool needs inputSchema");
        }
    }

    #[test]
    fn test_v4_codegen_files_to_disk() {
        let schema = full_ecommerce_schema();
        let dir = TempDir::new().unwrap();

        let result = generate_all(&schema, dir.path()).unwrap();
        assert_eq!(result.files.len(), 5);

        // All files should exist and be non-empty
        for file in &result.files {
            let path = dir.path().join(&file.filename);
            assert!(path.exists(), "{} should exist", file.filename);
            let content = std::fs::read_to_string(&path).unwrap();
            assert!(!content.is_empty(), "{} should not be empty", file.filename);
        }
    }

    #[test]
    fn test_v4_codegen_multiple_domains() {
        // Test that codegen works for various domain styles
        let domains = vec![
            "amazon.com",
            "best-buy.com",
            "docs.python.org",
            "my.site.co.uk",
        ];

        for domain in domains {
            let mut schema = test_schema();
            schema.domain = domain.to_string();
            let result = generate_all_in_memory(&schema);
            assert_eq!(
                result.files.len(),
                5,
                "Should generate 5 files for {domain}"
            );
            for file in &result.files {
                assert!(
                    !file.content.is_empty(),
                    "{} should not be empty for {domain}",
                    file.filename
                );
            }
        }
    }
}
