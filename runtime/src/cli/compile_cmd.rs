//! CLI handler for `cortex compile <domain>`.

use crate::cli::output;
use crate::compiler::{codegen, schema};
use crate::intelligence::cache::MapCache;
use anyhow::{bail, Result};
use std::path::PathBuf;

/// Run the compile command.
pub async fn run(domain: &str, _all: bool, output_dir: Option<&str>) -> Result<()> {
    // Load the cached map
    let mut cache = MapCache::default_cache()?;
    let site_map = match cache.load_map(domain)? {
        Some(map) => map,
        None => bail!("no cached map for '{domain}'. Run `cortex map {domain}` first."),
    };

    if !output::is_quiet() {
        println!("  Compiling {domain}...\n");
        println!(
            "  Analyzing map ({} nodes)...\n",
            site_map.header.node_count
        );
    }

    // Infer schema
    let compiled = schema::infer_schema(&site_map, domain);

    if !output::is_quiet() {
        println!("  Models:");
        for model in &compiled.models {
            println!(
                "    {:<16} {:>6} instances   {:>2} fields   {:>2} actions",
                model.name,
                model.instance_count,
                model.fields.len(),
                compiled
                    .actions
                    .iter()
                    .filter(|a| a.belongs_to == model.name)
                    .count()
            );
        }
        println!();
    }

    if !compiled.relationships.is_empty() && !output::is_quiet() {
        println!("  Relationships:");
        for rel in &compiled.relationships {
            println!(
                "    {} → {:<16} {:>12}  {:>6} edges",
                rel.from_model,
                rel.to_model,
                format!("{:?}", rel.cardinality).to_lowercase(),
                rel.edge_count
            );
        }
        println!();
    }

    // Determine output directory
    let out_dir = if let Some(dir) = output_dir {
        PathBuf::from(dir)
    } else {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".cortex").join("compiled").join(domain)
    };

    // Generate all files
    let files = codegen::generate_all(&compiled, &out_dir)?;

    if !output::is_quiet() {
        println!("  Generated:");
        println!("    {}/", out_dir.display());
        for file in &files.files {
            println!("    ├── {:<24} {:>5} B", file.filename, file.size);
        }
        println!();
        println!("  Usage:");

        let domain_safe = domain.replace(['.', '-'], "_");
        println!("    Python:     from cortex.compiled.{domain_safe} import Product, Cart");
        println!(
            "    TypeScript: import {{ searchProducts }} from '{}'",
            out_dir.join("client").display()
        );
        println!("    OpenAPI:    Point any REST client at the spec");
        println!("    GraphQL:    cortex graphql {domain} --port 7701");
    }

    if output::is_json() {
        output::print_json(&serde_json::json!({
            "domain": domain,
            "models": compiled.stats.total_models,
            "fields": compiled.stats.total_fields,
            "actions": compiled.actions.len(),
            "relationships": compiled.relationships.len(),
            "output_dir": out_dir.display().to_string(),
        }));
    }

    Ok(())
}
