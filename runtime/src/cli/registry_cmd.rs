//! CLI handlers for `cortex registry` subcommands.

use crate::cli::output;
use crate::collective::registry::LocalRegistry;
use anyhow::Result;
use std::path::PathBuf;

fn registry_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".cortex").join("registry")
}

/// List all maps in the local registry.
pub async fn run_list() -> Result<()> {
    let registry = LocalRegistry::new(registry_dir())?;
    let entries = registry.list();

    if entries.is_empty() {
        if !output::is_quiet() {
            println!("  Registry is empty. Maps are pushed here after `cortex map`.");
        }
        return Ok(());
    }

    if output::is_json() {
        let items: Vec<serde_json::Value> = entries
            .iter()
            .map(|e| {
                serde_json::json!({
                    "domain": e.domain,
                    "timestamp": e.latest_timestamp.to_rfc3339(),
                    "deltas": e.deltas.len(),
                })
            })
            .collect();
        output::print_json(&serde_json::json!({"entries": items}));
    } else {
        println!("  Registry entries:\n");
        for entry in &entries {
            println!(
                "    {:<30}  {} deltas  {}",
                entry.domain,
                entry.deltas.len(),
                entry.latest_timestamp.format("%Y-%m-%d %H:%M")
            );
        }
    }

    Ok(())
}

/// Show registry statistics.
pub async fn run_stats() -> Result<()> {
    let registry = LocalRegistry::new(registry_dir())?;
    let stats = registry.stats();

    if output::is_json() {
        output::print_json(&serde_json::json!(stats));
    } else {
        println!("  Registry stats:");
        println!("    Domains:    {}", stats.domain_count);
        println!("    Snapshots:  {} KB", stats.total_snapshot_bytes / 1024);
        println!("    Deltas:     {}", stats.total_deltas);
    }

    Ok(())
}

/// Garbage collect old deltas.
pub async fn run_gc() -> Result<()> {
    let mut registry = LocalRegistry::new(registry_dir())?;
    let removed = registry.gc(50)?; // keep last 50 deltas

    if output::is_json() {
        output::print_json(&serde_json::json!({"removed": removed}));
    } else {
        println!("  Removed {} old deltas.", removed);
    }

    Ok(())
}
