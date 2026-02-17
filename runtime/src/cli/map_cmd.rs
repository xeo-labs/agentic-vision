//! `cortex map <domain>` — map a website into a navigable graph.

use anyhow::Result;

/// Run the map command.
pub async fn run(domain: &str, max_nodes: u32, max_render: u32, timeout: u64) -> Result<()> {
    println!("cortex map: not yet implemented");
    println!("  domain:     {domain}");
    println!("  max_nodes:  {max_nodes}");
    println!("  max_render: {max_render}");
    println!("  timeout:    {timeout}ms");
    Ok(())
}
