//! `cortex pathfind <domain>` — find shortest path between nodes.

use anyhow::Result;

/// Run the pathfind command.
pub async fn run(domain: &str, from: u32, to: u32) -> Result<()> {
    println!("cortex pathfind: not yet implemented");
    println!("  domain: {domain}");
    println!("  from:   {from}");
    println!("  to:     {to}");
    Ok(())
}
