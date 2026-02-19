//! CLI handler for `cortex wql "<query>"`.

use crate::cli::output;
use crate::intelligence::cache::MapCache;
use crate::wql::{executor, parser, planner};
use anyhow::Result;

/// Run a WQL query.
pub async fn run(query_str: &str) -> Result<()> {
    // Parse the WQL query
    let query = parser::parse(query_str)?;

    // Create the execution plan
    let plan = planner::plan(&query, None)?;

    // Load all cached maps
    let mut cache = MapCache::default_cache()?;
    let maps = cache.load_all_maps()?;

    // Execute
    let rows = executor::execute(&plan, &maps)?;

    if output::is_json() {
        output::print_json(&serde_json::json!({
            "query": query_str,
            "results": rows.len(),
            "rows": rows,
        }));
    } else if rows.is_empty() {
        println!("  No results.");
    } else {
        // Print as table
        println!("  {} results:\n", rows.len());

        // Gather all field names
        let mut all_fields: Vec<String> = Vec::new();
        for row in &rows {
            for key in row.fields.keys() {
                if !all_fields.contains(key) {
                    all_fields.push(key.clone());
                }
            }
        }
        all_fields.sort();

        // Header
        print!("  {:<40}", "domain");
        for field in &all_fields {
            print!("  {:<16}", field);
        }
        println!();
        print!("  {}", "-".repeat(40));
        for _ in &all_fields {
            print!("  {}", "-".repeat(16));
        }
        println!();

        // Rows
        for row in &rows {
            print!("  {:<40}", row.domain);
            for field in &all_fields {
                let val = row
                    .fields
                    .get(field)
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string());
                print!("  {:<16}", val);
            }
            println!();
        }
    }

    Ok(())
}
