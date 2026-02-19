//! Core data types for the Web Compiler's compiled schema output.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A fully compiled schema for a single website domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledSchema {
    /// The domain this schema was compiled from.
    pub domain: String,
    /// When this schema was compiled.
    pub compiled_at: DateTime<Utc>,
    /// Discovered typed data models (Product, Article, etc.).
    pub models: Vec<DataModel>,
    /// Compiled HTTP actions (search, add_to_cart, etc.).
    pub actions: Vec<CompiledAction>,
    /// Relationships between models (belongs_to, has_many, etc.).
    pub relationships: Vec<ModelRelationship>,
    /// Compilation statistics.
    pub stats: SchemaStats,
}

/// A typed data model inferred from structured data on the site.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataModel {
    /// Human-readable name: "Product", "Article", etc.
    pub name: String,
    /// The Schema.org `@type` value this model corresponds to.
    pub schema_org_type: String,
    /// Fields discovered across all instances.
    pub fields: Vec<ModelField>,
    /// How many nodes matched this model type.
    pub instance_count: usize,
    /// Example URLs of this type (first 5).
    pub example_urls: Vec<String>,
    /// Search action for this model, if one exists.
    pub search_action: Option<CompiledAction>,
    /// Listing page URL for this type, if discovered.
    pub list_url: Option<String>,
}

/// A single field within a data model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelField {
    /// Field name: "price", "rating", "name", etc.
    pub name: String,
    /// Inferred type.
    pub field_type: FieldType,
    /// Where this field was discovered.
    pub source: FieldSource,
    /// Confidence in the field's type and value (0.0-1.0).
    pub confidence: f32,
    /// Whether this field is absent on some instances.
    pub nullable: bool,
    /// Example values seen (first 5 unique).
    pub example_values: Vec<String>,
    /// Feature vector dimension this field maps to, if any.
    pub feature_dim: Option<usize>,
}

/// Inferred type for a model field.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FieldType {
    String,
    Float,
    Integer,
    Bool,
    DateTime,
    Url,
    Enum(Vec<String>),
    Object(String),
    Array(Box<FieldType>),
}

impl FieldType {
    /// Convert to Python type annotation string.
    pub fn to_python_type(&self) -> String {
        match self {
            Self::String => "str".to_string(),
            Self::Float => "float".to_string(),
            Self::Integer => "int".to_string(),
            Self::Bool => "bool".to_string(),
            Self::DateTime => "datetime".to_string(),
            Self::Url => "str".to_string(),
            Self::Enum(variants) => {
                let joined = variants
                    .iter()
                    .map(|v| format!("\"{v}\""))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("Literal[{joined}]")
            }
            Self::Object(name) => format!("'{name}'"),
            Self::Array(inner) => format!("List[{}]", inner.to_python_type()),
        }
    }

    /// Convert to TypeScript type annotation string.
    pub fn to_ts_type(&self) -> String {
        match self {
            Self::String | Self::Url | Self::DateTime => "string".to_string(),
            Self::Float | Self::Integer => "number".to_string(),
            Self::Bool => "boolean".to_string(),
            Self::Enum(variants) => {
                let joined = variants
                    .iter()
                    .map(|v| format!("'{v}'"))
                    .collect::<Vec<_>>()
                    .join(" | ");
                joined
            }
            Self::Object(name) => name.clone(),
            Self::Array(inner) => format!("{}[]", inner.to_ts_type()),
        }
    }

    /// Convert to OpenAPI schema JSON type.
    pub fn to_openapi_type(&self) -> serde_json::Value {
        match self {
            Self::String | Self::Url | Self::DateTime => {
                serde_json::json!({"type": "string"})
            }
            Self::Float => serde_json::json!({"type": "number"}),
            Self::Integer => serde_json::json!({"type": "integer"}),
            Self::Bool => serde_json::json!({"type": "boolean"}),
            Self::Enum(variants) => {
                serde_json::json!({"type": "string", "enum": variants})
            }
            Self::Object(name) => {
                serde_json::json!({"$ref": format!("#/components/schemas/{name}")})
            }
            Self::Array(inner) => {
                serde_json::json!({"type": "array", "items": inner.to_openapi_type()})
            }
        }
    }

    /// Convert to GraphQL type name.
    pub fn to_graphql_type(&self) -> String {
        match self {
            Self::String | Self::Url | Self::DateTime => "String".to_string(),
            Self::Float => "Float".to_string(),
            Self::Integer => "Int".to_string(),
            Self::Bool => "Boolean".to_string(),
            Self::Enum(variants) => {
                // GraphQL enums need a name; we'll use context to generate one
                let _ = variants;
                "String".to_string()
            }
            Self::Object(name) => name.clone(),
            Self::Array(inner) => format!("[{}]", inner.to_graphql_type()),
        }
    }
}

/// Source of a discovered field.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FieldSource {
    /// From JSON-LD structured data (highest confidence: 0.99).
    JsonLd,
    /// From data-* HTML attributes (confidence: 0.95).
    DataAttribute,
    /// From meta tags (OG, Twitter, etc.) (confidence: 0.90).
    MetaTag,
    /// From CSS pattern engine selectors (confidence: 0.85).
    CssPattern,
    /// From ARIA labels (confidence: 0.80).
    AriaLabel,
    /// Inferred from feature vector values (confidence: 0.70).
    Inferred,
}

impl FieldSource {
    /// Default confidence for this source type.
    pub fn default_confidence(&self) -> f32 {
        match self {
            Self::JsonLd => 0.99,
            Self::DataAttribute => 0.95,
            Self::MetaTag => 0.90,
            Self::CssPattern => 0.85,
            Self::AriaLabel => 0.80,
            Self::Inferred => 0.70,
        }
    }
}

/// A compiled HTTP action (method) on a model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompiledAction {
    /// Method name: "add_to_cart", "search", "checkout", etc.
    pub name: String,
    /// Which model this action belongs to: "Product", "Cart", "Site".
    pub belongs_to: String,
    /// True if this action requires a specific node (instance method).
    pub is_instance_method: bool,
    /// HTTP method: "GET", "POST", etc.
    pub http_method: String,
    /// URL endpoint template: "/cart/add.js", "/s?k={query}".
    pub endpoint_template: String,
    /// Parameters for this action.
    pub params: Vec<ActionParam>,
    /// Whether authentication is required.
    pub requires_auth: bool,
    /// Execution path: "http", "websocket", "webmcp", "browser".
    pub execution_path: String,
    /// Confidence in the compiled action (0.0-1.0).
    pub confidence: f32,
}

/// A parameter for a compiled action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionParam {
    /// Parameter name.
    pub name: String,
    /// Inferred type.
    pub param_type: FieldType,
    /// Whether this parameter is required.
    pub required: bool,
    /// Default value, if known.
    pub default_value: Option<String>,
    /// Where this parameter appears: "form_field", "url_param", "json_body", "path_param".
    pub source: String,
}

/// A relationship between two models.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRelationship {
    /// Source model name.
    pub from_model: String,
    /// Target model name.
    pub to_model: String,
    /// Relationship name: "sold_by", "has_reviews", "similar_to".
    pub name: String,
    /// Cardinality.
    pub cardinality: Cardinality,
    /// Number of edges backing this relationship.
    pub edge_count: usize,
    /// Hint for graph traversal to navigate this relationship.
    pub traversal_hint: TraversalHint,
}

/// Cardinality of a model relationship.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Cardinality {
    /// Many-to-one.
    BelongsTo,
    /// One-to-many.
    HasMany,
    /// One-to-one.
    HasOne,
    /// Many-to-many.
    ManyToMany,
}

/// Hint for how to traverse a relationship in the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraversalHint {
    /// Edge types to follow.
    pub edge_types: Vec<String>,
    /// Whether to follow forward edges (from → to).
    pub forward: bool,
}

/// Compilation statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaStats {
    /// Number of models discovered.
    pub total_models: usize,
    /// Total fields across all models.
    pub total_fields: usize,
    /// Total node instances that matched models.
    pub total_instances: usize,
    /// Average confidence across all fields.
    pub avg_confidence: f32,
}

/// Files generated by the code generator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedFiles {
    /// Paths of generated files.
    pub files: Vec<GeneratedFile>,
}

/// A single generated file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedFile {
    /// Relative filename: "client.py", "openapi.yaml", etc.
    pub filename: String,
    /// Size in bytes.
    pub size: usize,
    /// Content of the file.
    pub content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_type_python_conversion() {
        assert_eq!(FieldType::String.to_python_type(), "str");
        assert_eq!(FieldType::Float.to_python_type(), "float");
        assert_eq!(FieldType::Integer.to_python_type(), "int");
        assert_eq!(FieldType::Bool.to_python_type(), "bool");
        assert_eq!(FieldType::Url.to_python_type(), "str");
        assert_eq!(FieldType::DateTime.to_python_type(), "datetime");
        assert_eq!(
            FieldType::Enum(vec!["a".into(), "b".into()]).to_python_type(),
            "Literal[\"a\", \"b\"]"
        );
        assert_eq!(
            FieldType::Array(Box::new(FieldType::String)).to_python_type(),
            "List[str]"
        );
    }

    #[test]
    fn test_field_type_typescript_conversion() {
        assert_eq!(FieldType::String.to_ts_type(), "string");
        assert_eq!(FieldType::Float.to_ts_type(), "number");
        assert_eq!(FieldType::Bool.to_ts_type(), "boolean");
        assert_eq!(
            FieldType::Enum(vec!["a".into(), "b".into()]).to_ts_type(),
            "'a' | 'b'"
        );
    }

    #[test]
    fn test_field_source_confidence() {
        assert_eq!(FieldSource::JsonLd.default_confidence(), 0.99);
        assert_eq!(FieldSource::DataAttribute.default_confidence(), 0.95);
        assert_eq!(FieldSource::Inferred.default_confidence(), 0.70);
    }

    #[test]
    fn test_field_type_openapi_conversion() {
        let val = FieldType::Float.to_openapi_type();
        assert_eq!(val["type"], "number");

        let val = FieldType::Enum(vec!["x".into(), "y".into()]).to_openapi_type();
        assert_eq!(val["type"], "string");
        assert_eq!(val["enum"][0], "x");
    }

    #[test]
    fn test_field_type_graphql_conversion() {
        assert_eq!(FieldType::String.to_graphql_type(), "String");
        assert_eq!(FieldType::Float.to_graphql_type(), "Float");
        assert_eq!(FieldType::Integer.to_graphql_type(), "Int");
        assert_eq!(FieldType::Bool.to_graphql_type(), "Boolean");
    }
}
