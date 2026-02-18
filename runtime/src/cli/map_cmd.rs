//! `cortex map <domain>` — map a website into a navigable graph.

use crate::cli::output::{self, Styled};
use crate::intelligence::cache::MapCache;
use crate::map::types::SiteMap;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::time::Instant;

/// Run the map command.
pub async fn run(
    domain: &str,
    max_nodes: u32,
    max_render: u32,
    timeout: u64,
    fresh: bool,
) -> Result<()> {
    let s = Styled::new();
    let start = Instant::now();

    // Check for cached map first (unless --fresh)
    if !fresh {
        let mut cache = MapCache::default_cache()?;
        if let Some(path) = cache.get(domain) {
            let data = std::fs::read(path)?;
            let map = SiteMap::deserialize(&data).context("failed to load cached map")?;

            if output::is_json() {
                print_map_json(&map, start.elapsed());
                return Ok(());
            }

            if !output::is_quiet() {
                let age = path
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.elapsed().ok())
                    .map(|d| output::format_duration(d.as_secs()))
                    .unwrap_or_else(|| "unknown".to_string());
                eprintln!("  Using cached map ({age} old). Use --fresh to re-map.");
                eprintln!();
            }

            print_map_stats(&s, &map, start.elapsed());
            return Ok(());
        }
    }

    // First-run auto-setup: install Chromium and start daemon if needed
    let show_progress = !output::is_quiet() && !output::is_json();

    if show_progress {
        eprintln!();
        eprintln!("  {} Mapping {domain}", s.bold("CORTEX —"));
        eprintln!();
    }

    let needs_setup = auto_setup_if_needed().await?;
    if needs_setup && show_progress {
        eprintln!();
    }

    // Connect to the daemon socket and send a MAP request
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixStream;

    let socket_path = "/tmp/cortex.sock";
    let mut stream = match UnixStream::connect(socket_path).await {
        Ok(s) => s,
        Err(_) => {
            if output::is_json() {
                output::print_json(&serde_json::json!({
                    "status": "daemon_required",
                    "message": "Cannot connect to Cortex daemon",
                    "hint": "Start with: cortex start"
                }));
            } else if !output::is_quiet() {
                eprintln!("  Cannot connect to Cortex daemon.");
                eprintln!("  Start the daemon with: cortex start");
            }
            return Ok(());
        }
    };

    // Spawn a background SSE listener for live progress (best effort)
    let sse_domain = domain.to_string();
    let sse_handle = if show_progress {
        Some(tokio::spawn(stream_progress_from_sse(sse_domain)))
    } else {
        None
    };

    let req = serde_json::json!({
        "id": format!("map-{}", std::process::id()),
        "method": "map",
        "params": {
            "domain": domain,
            "max_nodes": max_nodes,
            "max_render": max_render,
            "max_time_ms": timeout,
            "respect_robots": true,
        }
    });
    let req_str = format!("{}\n", req);
    stream
        .write_all(req_str.as_bytes())
        .await
        .context("failed to send MAP request")?;

    // Read response (with generous timeout for mapping)
    let (reader, _writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    let response_timeout = std::time::Duration::from_millis(timeout + 30000);
    let read_result = tokio::time::timeout(response_timeout, reader.read_line(&mut line)).await;

    // Cancel the SSE listener
    if let Some(handle) = sse_handle {
        handle.abort();
    }

    match read_result {
        Ok(Ok(n)) if n > 0 => {} // Data received into `line`
        Ok(Ok(_)) => {
            if !output::is_quiet() {
                eprintln!("  Connection closed by server.");
            }
            return Ok(());
        }
        Ok(Err(e)) => {
            if !output::is_quiet() {
                eprintln!("  Read error: {e}");
            }
            return Ok(());
        }
        Err(_) => {
            if !output::is_quiet() {
                eprintln!("  Mapping timed out after {}ms.", timeout + 30000);
            }
            return Ok(());
        }
    };

    let response: serde_json::Value =
        serde_json::from_str(line.trim()).context("failed to parse response")?;

    if let Some(error) = response.get("error") {
        if output::is_json() {
            output::print_json(&response);
        } else if !output::is_quiet() {
            let msg = error
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            eprintln!("  Mapping failed: {msg}");
        }
        return Ok(());
    }

    let result = response.get("result").cloned().unwrap_or_default();
    let node_count = result
        .get("node_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let edge_count = result
        .get("edge_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    if output::is_json() {
        output::print_json(&result);
        return Ok(());
    }

    if show_progress {
        let elapsed = start.elapsed();
        eprintln!();
        eprintln!("  {} Mapped {domain}", s.ok_sym());
        eprintln!(
            "    {} nodes  ·  {} edges  ·  {:.1}s",
            format_count(node_count),
            format_count(edge_count),
            elapsed.as_secs_f64()
        );
        eprintln!();
        eprintln!("  Query with: cortex query {domain} --type product_detail");

        // Show dashboard hint if HTTP is available
        eprintln!("  Dashboard:  http://localhost:7700/dashboard");
    }

    // Cache the map binary if available
    if let Some(map_path) = result.get("map_path").and_then(|v| v.as_str()) {
        if show_progress {
            eprintln!("  Cached at:  {map_path}");
        }
    }

    Ok(())
}

/// Print map stats in branded format.
fn print_map_stats(s: &Styled, map: &SiteMap, elapsed: std::time::Duration) {
    let rendered = map.nodes.iter().filter(|n| n.flags.is_rendered()).count();
    let estimated = map.nodes.len() - rendered;

    // Count page types
    let mut type_counts: HashMap<String, usize> = HashMap::new();
    for node in &map.nodes {
        let name = format!("{:?}", node.page_type).to_lowercase();
        *type_counts.entry(name).or_default() += 1;
    }
    let mut type_vec: Vec<(String, usize)> = type_counts.into_iter().collect();
    type_vec.sort_by(|a, b| b.1.cmp(&a.1));

    let total_nodes = map.nodes.len();

    eprintln!("  Map complete in {:.1}s", elapsed.as_secs_f64());
    eprintln!();
    eprintln!("  {}", s.bold(&format!("{:<45}", map.header.domain)));
    eprintln!(
        "  Nodes:     {} ({} rendered, {} estimated)",
        total_nodes, rendered, estimated
    );
    eprintln!("  Edges:     {}", map.edges.len());
    if !map.cluster_centroids.is_empty() {
        eprintln!("  Clusters:  {}", map.cluster_centroids.len());
    }
    eprintln!("  Actions:   {}", map.actions.len());
    eprintln!();

    if !type_vec.is_empty() {
        eprintln!("  Top page types:");
        for (name, count) in type_vec.iter().take(5) {
            let pct = if total_nodes > 0 {
                (count * 100) / total_nodes
            } else {
                0
            };
            eprintln!("    {:<20} {:>6}  ({pct}%)", name, count);
        }
    }

    eprintln!();
    eprintln!(
        "  Query with: cortex query {} --type product_detail",
        map.header.domain
    );
}

/// Print map stats as JSON.
fn print_map_json(map: &SiteMap, elapsed: std::time::Duration) {
    let rendered = map.nodes.iter().filter(|n| n.flags.is_rendered()).count();

    let mut type_counts: HashMap<String, usize> = HashMap::new();
    for node in &map.nodes {
        let name = format!("{:?}", node.page_type).to_lowercase();
        *type_counts.entry(name).or_default() += 1;
    }

    output::print_json(&serde_json::json!({
        "domain": map.header.domain,
        "nodes": map.nodes.len(),
        "edges": map.edges.len(),
        "rendered": rendered,
        "clusters": map.cluster_centroids.len(),
        "actions": map.actions.len(),
        "page_types": type_counts,
        "duration_ms": elapsed.as_millis(),
    }));
}

/// Auto-install Chromium and auto-start daemon if needed.
///
/// Returns `true` if any setup action was taken, `false` if already ready.
///
/// This implements the "first-run experience" from Section 6 of the edge cases doc:
/// `cortex map` should auto-install and auto-start so users never need to run
/// `cortex install` and `cortex start` separately on first use.
async fn auto_setup_if_needed() -> Result<bool> {
    let mut did_something = false;

    // Check if Chromium is installed
    let chromium_path = crate::cli::doctor::find_chromium();
    if chromium_path.is_none() {
        if !output::is_quiet() {
            let s = Styled::new();
            eprintln!(
                "  {} Cortex is not set up yet. Let's get you started:",
                s.info_sym()
            );
            eprintln!();
            eprintln!("  [1/2] Installing Chromium...");
        }
        // Try to install
        match crate::cli::install_cmd::run_with_force(false).await {
            Ok(()) => {
                did_something = true;
            }
            Err(e) => {
                // Installation failed — give clear instructions
                if !output::is_quiet() {
                    eprintln!("    Failed to auto-install Chromium: {e}");
                    eprintln!("    Run 'cortex install' manually for detailed output.");
                }
                return Err(e);
            }
        }
    }

    // Check if daemon is running
    let socket_path = std::path::PathBuf::from("/tmp/cortex.sock");
    if !socket_path.exists() {
        if !output::is_quiet() {
            if did_something {
                eprintln!("  [2/2] Starting Cortex process...");
            } else {
                let s = Styled::new();
                eprintln!("  {} Cortex is not running. Starting...", s.info_sym());
            }
        }
        match crate::cli::start::run().await {
            Ok(()) => {
                did_something = true;
                // Brief pause for the daemon to initialize
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            }
            Err(e) => {
                if !output::is_quiet() {
                    eprintln!("    Failed to auto-start: {e}");
                    eprintln!("    Run 'cortex start' manually for details.");
                }
                return Err(e);
            }
        }
    }

    Ok(did_something)
}

/// Format a number with comma separators for display.
fn format_count(n: u64) -> String {
    if n < 1_000 {
        return n.to_string();
    }
    let s = n.to_string();
    let mut result = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}

/// Subscribe to SSE events and print live mapping progress.
///
/// Best-effort: if the REST API is not running, this silently returns.
/// Runs concurrently with the socket MAP request. Uses a raw TCP
/// connection to avoid additional crate dependencies for streaming.
async fn stream_progress_from_sse(domain: String) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    // Connect to the REST API
    let mut stream = match TcpStream::connect("127.0.0.1:7700").await {
        Ok(s) => s,
        Err(_) => return, // REST API not available — skip live progress
    };

    // Send HTTP GET request for SSE
    let request = format!(
        "GET /api/v1/events?domain={domain} HTTP/1.1\r\n\
         Host: 127.0.0.1:7700\r\n\
         Accept: text/event-stream\r\n\
         Connection: keep-alive\r\n\
         \r\n"
    );
    if stream.write_all(request.as_bytes()).await.is_err() {
        return;
    }

    let s = Styled::new();
    let reader = tokio::io::BufReader::new(stream);
    let mut lines = reader.lines();

    // Skip HTTP response headers
    let mut past_headers = false;
    while let Ok(Some(line)) = lines.next_line().await {
        if line.is_empty() {
            past_headers = true;
            break;
        }
    }
    if !past_headers {
        return;
    }

    // Read SSE data lines
    while let Ok(Some(line)) = lines.next_line().await {
        let trimmed = line.trim();
        if !trimmed.starts_with("data:") {
            continue;
        }
        let json_str = trimmed.trim_start_matches("data:").trim();
        if json_str.is_empty() {
            continue;
        }

        let event: serde_json::Value = match serde_json::from_str(json_str) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let event_type = event.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match event_type {
            "SitemapDiscovered" => {
                let count = event.get("url_count").and_then(|v| v.as_u64()).unwrap_or(0);
                let ms = event
                    .get("elapsed_ms")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                eprintln!(
                    "  Layer 0  {:<22} {} URLs discovered {:>20} {}",
                    "Metadata",
                    format_count(count),
                    format!("{:.1}s", ms as f64 / 1000.0),
                    s.ok_sym(),
                );
            }
            "StructuredDataExtracted" => {
                let pages = event
                    .get("pages_fetched")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let jsonld = event
                    .get("jsonld_found")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let patterns = event
                    .get("patterns_used")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let ms = event
                    .get("elapsed_ms")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let pct = if pages > 0 { (jsonld * 100) / pages } else { 0 };
                eprintln!(
                    "  Layer 1  {:<22} {} pages, {} JSON-LD ({}%) {:>15} {}",
                    "Structured Data",
                    pages,
                    jsonld,
                    pct,
                    format!("{:.1}s", ms as f64 / 1000.0),
                    s.ok_sym(),
                );
                if patterns > 0 {
                    eprintln!(
                        "  Layer 1½ {:<22} {} pages enriched via CSS selectors",
                        "Pattern Engine", patterns,
                    );
                }
            }
            "LayerComplete" => {
                let layer = event.get("layer").and_then(|v| v.as_u64()).unwrap_or(0);
                let name = event
                    .get("layer_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let ms = event
                    .get("elapsed_ms")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                eprintln!(
                    "  Layer {}  {:<22} {:>36} {}",
                    layer,
                    name,
                    format!("{:.1}s", ms as f64 / 1000.0),
                    s.ok_sym(),
                );
            }
            "MapComplete" | "MapFailed" => {
                // Stop streaming — the final summary comes from the socket response
                break;
            }
            _ => {} // Skip other event types
        }
    }
}
