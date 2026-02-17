//! `cortex pathfind <domain>` — find shortest path between nodes.

use crate::cli::output::{self, Styled};
use crate::intelligence::cache::MapCache;
use crate::map::types::PathConstraints;
use anyhow::{bail, Result};

/// Run the pathfind command.
pub async fn run(domain: &str, from: u32, to: u32) -> Result<()> {
    let s = Styled::new();

    // Load cached map
    let mut cache = MapCache::default_cache()?;
    let map = match cache.load_map(domain)? {
        Some(m) => m,
        None => {
            if output::is_json() {
                output::print_json(&serde_json::json!({
                    "error": "no_map",
                    "message": format!("No cached map for '{domain}'"),
                    "hint": format!("Run: cortex map {domain}")
                }));
                return Ok(());
            }
            bail!("No map found for '{domain}'. Run 'cortex map {domain}' first.");
        }
    };

    // Validate node indices
    let max_node = map.nodes.len() as u32;
    if from >= max_node {
        bail!("Source node {from} doesn't exist in this map (max: {}).", max_node - 1);
    }
    if to >= max_node {
        bail!("Target node {to} doesn't exist in this map (max: {}).", max_node - 1);
    }

    let constraints = PathConstraints::default();
    let path = map.shortest_path(from, to, &constraints);

    if output::is_json() {
        match &path {
            Some(p) => {
                let nodes: Vec<serde_json::Value> = p
                    .nodes
                    .iter()
                    .map(|&idx| {
                        let url = map.urls.get(idx as usize).cloned().unwrap_or_default();
                        let node = &map.nodes[idx as usize];
                        serde_json::json!({
                            "index": idx,
                            "url": url,
                            "page_type": format!("{:?}", node.page_type),
                        })
                    })
                    .collect();
                output::print_json(&serde_json::json!({
                    "found": true,
                    "hops": p.hops,
                    "total_weight": p.total_weight,
                    "nodes": nodes,
                    "required_actions": p.required_actions.len(),
                }));
            }
            None => {
                output::print_json(&serde_json::json!({
                    "found": false,
                    "from": from,
                    "to": to,
                }));
            }
        }
        return Ok(());
    }

    match path {
        Some(path) => {
            if !output::is_quiet() {
                eprintln!("  Path from node {from} to node {to}:");
                eprintln!("  Hops:   {}", path.hops);
                eprintln!("  Weight: {:.2}", path.total_weight);

                if path.hops > 20 {
                    eprintln!();
                    eprintln!(
                        "  {} Path has {} hops. This seems unusually long.",
                        s.warn_sym(),
                        path.hops
                    );
                }

                eprintln!();
                eprintln!("  Route:");
                for (i, &node_idx) in path.nodes.iter().enumerate() {
                    let url = map
                        .urls
                        .get(node_idx as usize)
                        .map(|s| s.as_str())
                        .unwrap_or("?");
                    let node = &map.nodes[node_idx as usize];
                    let arrow = if i < path.nodes.len() - 1 {
                        " \u{2192}"
                    } else {
                        "  "
                    };
                    eprintln!(
                        "    [{node_idx:>5}] {:?} {url}{arrow}",
                        node.page_type
                    );
                }

                if !path.required_actions.is_empty() {
                    eprintln!();
                    eprintln!("  Required actions:");
                    for action in &path.required_actions {
                        eprintln!(
                            "    At node {}: opcode ({:#04x}, {:#04x})",
                            action.at_node, action.opcode.category, action.opcode.action
                        );
                    }
                }
            }
        }
        None => {
            if !output::is_quiet() {
                eprintln!(
                    "  No path found from node {from} to node {to}."
                );
                eprintln!(
                    "  Try relaxing constraints or checking that both nodes are connected."
                );
            }
        }
    }

    Ok(())
}
