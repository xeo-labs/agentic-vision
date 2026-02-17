//! `cortex pathfind <domain>` — find shortest path between nodes.

use crate::intelligence::cache::MapCache;
use crate::map::types::PathConstraints;
use anyhow::{Context, Result};

/// Run the pathfind command.
pub async fn run(domain: &str, from: u32, to: u32) -> Result<()> {
    // Load cached map
    let cache = MapCache::default_cache()?;
    let map = cache
        .load_map(domain)?
        .ok_or_else(|| anyhow::anyhow!("No cached map for {domain}. Run: cortex map {domain}"))?;

    let constraints = PathConstraints::default();
    let path = map.shortest_path(from, to, &constraints);

    match path {
        Some(path) => {
            println!("Path from node {from} to node {to}:");
            println!("  Hops:   {}", path.hops);
            println!("  Weight: {:.2}", path.total_weight);
            println!("  Nodes:");
            for &node_idx in &path.nodes {
                let url = map
                    .urls
                    .get(node_idx as usize)
                    .map(|s| s.as_str())
                    .unwrap_or("?");
                let node = &map.nodes[node_idx as usize];
                println!("    [{node_idx:>4}] {:?} {url}", node.page_type);
            }
            if !path.required_actions.is_empty() {
                println!("  Required actions:");
                for action in &path.required_actions {
                    println!(
                        "    At node {}: opcode ({:#04x}, {:#04x})",
                        action.at_node, action.opcode.category, action.opcode.action
                    );
                }
            }
        }
        None => {
            println!("No path found from node {from} to node {to}.");
        }
    }

    Ok(())
}
