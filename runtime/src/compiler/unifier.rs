//! Cross-site schema unification via Schema.org types.
//!
//! Schema.org types are universal — Product on Amazon = Product on Best Buy.
//! The unifier merges compiled schemas from multiple sites into a unified schema
//! where queries can span all compiled domains.

use crate::compiler::models::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};

/// A unified schema spanning multiple compiled sites.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedSchema {
    /// Unified models (one per Schema.org type, combining all domains).
    pub models: Vec<UnifiedModel>,
    /// All domains included in this unified schema.
    pub domains: Vec<String>,
    /// Total node instances across all domains.
    pub total_instances: usize,
}

/// A unified model combining instances from multiple domains.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedModel {
    /// Model name (e.g., "Product").
    pub name: String,
    /// Schema.org type.
    pub schema_org_type: String,
    /// Union of all fields across all domains.
    pub fields: Vec<UnifiedField>,
    /// Which domains contribute to this model.
    pub sources: Vec<ModelSource>,
    /// Total instances across all domains.
    pub total_instances: usize,
}

/// A field in a unified model — tracks which domains have it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedField {
    /// Canonical Schema.org property name.
    pub canonical_name: String,
    /// Inferred type.
    pub field_type: FieldType,
    /// Which domains have this field.
    pub present_in: Vec<String>,
    /// Percentage of sources that have this field (0.0-1.0).
    pub coverage: f32,
}

/// Info about a domain's contribution to a unified model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSource {
    /// Domain name.
    pub domain: String,
    /// Number of instances of this model on this domain.
    pub instance_count: usize,
    /// What percentage of the unified fields this domain has.
    pub field_coverage: f32,
}

/// Unify multiple compiled schemas into a single unified schema.
///
/// Groups models by Schema.org type, unions all fields, normalizes names,
/// and records which domains contribute what.
pub fn unify_schemas(schemas: &[CompiledSchema]) -> UnifiedSchema {
    if schemas.is_empty() {
        return UnifiedSchema {
            models: Vec::new(),
            domains: Vec::new(),
            total_instances: 0,
        };
    }

    let domains: Vec<String> = schemas.iter().map(|s| s.domain.clone()).collect();
    let domain_count = domains.len();

    // Group all models by their schema_org_type
    let mut type_groups: HashMap<String, Vec<(&str, &DataModel)>> = HashMap::new();
    for schema in schemas {
        for model in &schema.models {
            type_groups
                .entry(model.schema_org_type.clone())
                .or_default()
                .push((&schema.domain, model));
        }
    }

    let mut unified_models: Vec<UnifiedModel> = Vec::new();

    for (schema_type, models) in &type_groups {
        // Union all fields
        let mut field_map: HashMap<String, (FieldType, BTreeSet<String>)> = HashMap::new();

        for (domain, model) in models {
            for field in &model.fields {
                let canonical = canonicalize_field_name(&field.name);
                let entry = field_map
                    .entry(canonical.clone())
                    .or_insert_with(|| (field.field_type.clone(), BTreeSet::new()));
                entry.1.insert(domain.to_string());
            }
        }

        let unified_fields: Vec<UnifiedField> = field_map
            .into_iter()
            .map(|(name, (field_type, present_in))| {
                let coverage = present_in.len() as f32 / domain_count as f32;
                UnifiedField {
                    canonical_name: name,
                    field_type,
                    present_in: present_in.into_iter().collect(),
                    coverage,
                }
            })
            .collect();

        let unified_field_count = unified_fields.len();

        // Build sources
        let sources: Vec<ModelSource> = models
            .iter()
            .map(|(domain, model)| {
                let field_coverage = if unified_field_count > 0 {
                    model.fields.len() as f32 / unified_field_count as f32
                } else {
                    0.0
                };
                ModelSource {
                    domain: domain.to_string(),
                    instance_count: model.instance_count,
                    field_coverage,
                }
            })
            .collect();

        let total_instances: usize = models.iter().map(|(_, m)| m.instance_count).sum();

        // Use the most common model name
        let name = models[0].1.name.clone();

        unified_models.push(UnifiedModel {
            name,
            schema_org_type: schema_type.clone(),
            fields: unified_fields,
            sources,
            total_instances,
        });
    }

    // Sort by total instances (most significant first)
    unified_models.sort_by(|a, b| b.total_instances.cmp(&a.total_instances));

    let total_instances: usize = unified_models.iter().map(|m| m.total_instances).sum();

    UnifiedSchema {
        models: unified_models,
        domains,
        total_instances,
    }
}

/// Canonicalize a field name to Schema.org conventions.
fn canonicalize_field_name(name: &str) -> String {
    // Map common aliases to canonical names
    match name {
        "price" | "cost" | "amount" => "price".to_string(),
        "rating" | "score" | "stars" => "rating".to_string(),
        "name" | "title" | "label" => "name".to_string(),
        "url" | "link" | "href" => "url".to_string(),
        "node_id" | "id" | "nodeId" => "node_id".to_string(),
        "image_url" | "image" | "thumbnail" | "picture" => "image_url".to_string(),
        "description" | "desc" | "summary" | "blurb" => "description".to_string(),
        "category" | "type" | "kind" => "category".to_string(),
        "brand" | "manufacturer" | "maker" => "brand".to_string(),
        "availability" | "in_stock" | "stock" => "availability".to_string(),
        other => other.to_string(),
    }
}

/// Generate a universal Python client that queries all compiled sites.
pub fn generate_universal_python(unified: &UnifiedSchema) -> String {
    let mut out = String::new();

    out.push_str("\"\"\"Universal Cortex client — queries all compiled sites\"\"\"\n");
    out.push_str("# Generated by Cortex Web Compiler — do not edit manually\n\n");
    out.push_str("from __future__ import annotations\n");
    out.push_str("from dataclasses import dataclass, field\n");
    out.push_str("from typing import Optional, List, Any\n");
    out.push_str("import importlib\n\n");

    // Domain registry
    out.push_str("_COMPILED_DOMAINS = [\n");
    for domain in &unified.domains {
        let module = domain.replace(['.', '-'], "_");
        out.push_str(&format!(
            "    (\"{domain}\", \"cortex.compiled.{module}\"),\n"
        ));
    }
    out.push_str("]\n\n");

    out.push_str(
        r#"def _get_compiled_sites(domains=None):
    """Load compiled site modules."""
    sites = []
    for domain, module_name in _COMPILED_DOMAINS:
        if domains and domain not in domains:
            continue
        try:
            mod = importlib.import_module(module_name)
            sites.append((domain, mod))
        except ImportError:
            continue
    return sites

"#,
    );

    // Generate unified dataclasses
    for model in &unified.models {
        out.push_str(&format!("\n@dataclass\nclass {}:\n", model.name));
        out.push_str(&format!(
            "    \"\"\"Unified {} from {} sites\"\"\"\n",
            model.name,
            model.sources.len()
        ));
        out.push_str("    url: str\n");
        out.push_str("    source_domain: str\n");

        for field in &model.fields {
            if field.canonical_name == "url" || field.canonical_name == "node_id" {
                continue;
            }
            let py_type = field.field_type.to_python_type();
            if field.coverage < 1.0 {
                out.push_str(&format!(
                    "    {}: Optional[{}] = None\n",
                    field.canonical_name, py_type
                ));
            } else {
                out.push_str(&format!(
                    "    {}: {} = \"\"\n",
                    field.canonical_name, py_type
                ));
            }
        }

        // search method
        out.push_str(&format!(
            r#"
    @staticmethod
    def search(query: str, domains: List[str] = None, **filters) -> List[{name}]:
        """Search {name}s across all compiled sites."""
        results = []
        for domain, site_module in _get_compiled_sites(domains):
            try:
                if hasattr(site_module, '{name}'):
                    site_cls = getattr(site_module, '{name}')
                    site_results = site_cls.search(query, **filters)
                    results.extend([
                        {name}(url=r.url, source_domain=domain, **{{
                            k: getattr(r, k, None) for k in {name}.__dataclass_fields__
                            if k not in ('url', 'source_domain')
                        }})
                        for r in site_results
                    ])
            except Exception:
                continue
        return results

"#,
            name = model.name
        ));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_schema(domain: &str, models: Vec<DataModel>) -> CompiledSchema {
        CompiledSchema {
            domain: domain.to_string(),
            compiled_at: Utc::now(),
            models,
            actions: Vec::new(),
            relationships: Vec::new(),
            stats: SchemaStats {
                total_models: 0,
                total_fields: 0,
                total_instances: 0,
                avg_confidence: 0.0,
            },
        }
    }

    fn product_model(instance_count: usize) -> DataModel {
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
            instance_count,
            example_urls: vec![],
            search_action: None,
            list_url: None,
        }
    }

    #[test]
    fn test_unify_two_schemas() {
        let schemas = vec![
            make_schema("amazon.com", vec![product_model(50000)]),
            make_schema("bestbuy.com", vec![product_model(20000)]),
        ];

        let unified = unify_schemas(&schemas);
        assert_eq!(unified.domains.len(), 2);
        assert_eq!(unified.models.len(), 1);

        let product = &unified.models[0];
        assert_eq!(product.name, "Product");
        assert_eq!(product.total_instances, 70000);
        assert_eq!(product.sources.len(), 2);
    }

    #[test]
    fn test_unify_empty() {
        let unified = unify_schemas(&[]);
        assert!(unified.models.is_empty());
        assert_eq!(unified.total_instances, 0);
    }

    #[test]
    fn test_canonicalize_field_name() {
        assert_eq!(canonicalize_field_name("price"), "price");
        assert_eq!(canonicalize_field_name("cost"), "price");
        assert_eq!(canonicalize_field_name("rating"), "rating");
        assert_eq!(canonicalize_field_name("stars"), "rating");
        assert_eq!(canonicalize_field_name("title"), "name");
    }

    #[test]
    fn test_unified_field_coverage() {
        let schemas = vec![
            make_schema("a.com", vec![product_model(100)]),
            make_schema("b.com", vec![product_model(200)]),
        ];

        let unified = unify_schemas(&schemas);
        let product = &unified.models[0];

        // All fields present in both sites → coverage = 1.0
        for field in &product.fields {
            assert_eq!(
                field.coverage, 1.0,
                "field {} should have full coverage",
                field.canonical_name
            );
        }
    }

    #[test]
    fn test_generate_universal_python() {
        let schemas = vec![
            make_schema("amazon.com", vec![product_model(50000)]),
            make_schema("bestbuy.com", vec![product_model(20000)]),
        ];
        let unified = unify_schemas(&schemas);
        let code = generate_universal_python(&unified);

        assert!(code.contains("class Product:"));
        assert!(code.contains("source_domain: str"));
        assert!(code.contains("def search("));
        assert!(code.contains("_COMPILED_DOMAINS"));
    }

    // ── v4 Test Suite: Phase 1C — Cross-Site Unification ──

    #[test]
    fn test_v4_unify_many_domains() {
        let schemas: Vec<CompiledSchema> = (0..10)
            .map(|i| make_schema(&format!("site{i}.com"), vec![product_model(100 * (i + 1))]))
            .collect();

        let unified = unify_schemas(&schemas);
        assert_eq!(unified.domains.len(), 10);
        assert_eq!(unified.models.len(), 1);

        let product = &unified.models[0];
        assert_eq!(product.sources.len(), 10);
        assert_eq!(product.total_instances, 5500); // sum of 100, 200, ..., 1000
    }

    #[test]
    fn test_v4_unify_different_model_types() {
        let article_model = DataModel {
            name: "Article".to_string(),
            schema_org_type: "Article".to_string(),
            fields: vec![ModelField {
                name: "title".to_string(),
                field_type: FieldType::String,
                source: FieldSource::JsonLd,
                confidence: 0.95,
                nullable: false,
                example_values: vec![],
                feature_dim: None,
            }],
            instance_count: 1000,
            example_urls: vec![],
            search_action: None,
            list_url: None,
        };

        let schemas = vec![
            make_schema("amazon.com", vec![product_model(5000)]),
            make_schema("bbc.com", vec![article_model.clone()]),
            make_schema("bestbuy.com", vec![product_model(3000)]),
            make_schema("cnn.com", vec![article_model]),
        ];

        let unified = unify_schemas(&schemas);
        assert_eq!(unified.domains.len(), 4);
        assert_eq!(unified.models.len(), 2, "should have Product and Article");

        let product = unified.models.iter().find(|m| m.name == "Product").unwrap();
        assert_eq!(product.sources.len(), 2);
        assert_eq!(product.total_instances, 8000);

        let article = unified.models.iter().find(|m| m.name == "Article").unwrap();
        assert_eq!(article.sources.len(), 2);
        assert_eq!(article.total_instances, 2000);
    }

    #[test]
    fn test_v4_universal_python_multi_type() {
        let article_model = DataModel {
            name: "Article".to_string(),
            schema_org_type: "Article".to_string(),
            fields: vec![ModelField {
                name: "title".to_string(),
                field_type: FieldType::String,
                source: FieldSource::JsonLd,
                confidence: 0.9,
                nullable: false,
                example_values: vec![],
                feature_dim: None,
            }],
            instance_count: 500,
            example_urls: vec![],
            search_action: None,
            list_url: None,
        };

        let schemas = vec![
            make_schema("amazon.com", vec![product_model(5000)]),
            make_schema("bbc.com", vec![article_model]),
        ];

        let unified = unify_schemas(&schemas);
        let code = generate_universal_python(&unified);

        assert!(code.contains("class Product:"), "Product class");
        assert!(code.contains("class Article:"), "Article class");
        assert!(code.contains("_COMPILED_DOMAINS"), "domain registry");
    }
}
