//! `cortex map <domain>` — map a website into a navigable graph.

use crate::intelligence::cache::MapCache;
use anyhow::{Context, Result};
use std::time::Instant;

/// Run the map command.
pub async fn run(domain: &str, max_nodes: u32, max_render: u32, timeout: u64) -> Result<()> {
    let start = Instant::now();
    println!("Mapping {domain}...");
    println!("  max_nodes:  {max_nodes}");
    println!("  max_render: {max_render}");
    println!("  timeout:    {timeout}ms");

    // Check for cached map first
    let cache = MapCache::default_cache()?;
    if let Some(path) = cache.get(domain) {
        println!("\nUsing cached map: {}", path.display());
        let data = std::fs::read(path)?;
        let map = crate::map::types::SiteMap::deserialize(&data)
            .context("failed to load cached map")?;
        print_map_stats(&map, start.elapsed());
        return Ok(());
    }

    // TODO: When browser pool is wired up, perform actual mapping here.
    // For now, print a helpful message.
    println!("\nMap command requires a running Cortex daemon with browser pool.");
    println!("Start the daemon with: cortex start");
    println!("Then run: cortex map {domain}");
    println!("\nDuration: {:.2}s", start.elapsed().as_secs_f64());

    Ok(())
}

fn print_map_stats(map: &crate::map::types::SiteMap, elapsed: std::time::Duration) {
    let rendered = map.nodes.iter().filter(|n| n.flags.is_rendered()).count();
    println!("\nMap complete:");
    println!("  Domain:   {}", map.header.domain);
    println!("  Nodes:    {}", map.nodes.len());
    println!("  Edges:    {}", map.edges.len());
    println!("  Rendered: {rendered}");
    println!("  Actions:  {}", map.actions.len());
    println!("  Duration: {:.2}s", elapsed.as_secs_f64());
}
