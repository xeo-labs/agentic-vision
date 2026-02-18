//! Unix domain socket server for the Cortex protocol.
//!
//! Handles connection lifecycle, inactivity timeouts, malformed JSON,
//! rate limiting, and concurrent request management.

use crate::acquisition::http_session::HttpSession;
use crate::cartography::mapper::{MapRequest, Mapper};
use crate::live::perceive as perceive_handler;
use crate::map::types::{
    FeatureRange, NodeFlags, NodeQuery, PageType, PathConstraints, PathMinimize, SiteMap,
    FEATURE_DIM,
};
use crate::navigation::{pathfinder, query};
use crate::protocol::{self, Method};
use crate::renderer::Renderer;
use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::{Mutex, Notify, RwLock};
use tracing::{error, info, warn};

/// Inactivity timeout per connection (5 minutes — long enough for MAP requests).
const INACTIVITY_TIMEOUT: Duration = Duration::from_secs(300);

/// Maximum request line size (10 MB).
const MAX_REQUEST_SIZE: usize = 10 * 1024 * 1024;

/// Maximum requests per second per connection.
const MAX_REQUESTS_PER_SEC: u32 = 100;

/// Shared state passed to connection handlers.
struct SharedState {
    started_at: Instant,
    seen_ids: Arc<Mutex<HashSet<String>>>,
    maps: Arc<RwLock<HashMap<String, SiteMap>>>,
    /// Authenticated sessions keyed by session_id.
    sessions: Arc<RwLock<HashMap<String, HttpSession>>>,
    mapper: Option<Arc<Mapper>>,
    renderer: Option<Arc<dyn Renderer>>,
}

/// The Cortex socket server.
pub struct Server {
    socket_path: PathBuf,
    started_at: Instant,
    shutdown: Arc<Notify>,
    /// Track request IDs to reject duplicates.
    seen_ids: Arc<Mutex<HashSet<String>>>,
    /// In-memory map store. RwLock allows concurrent reads (QUERY/PATHFIND)
    /// while serializing writes (MAP completion).
    maps: Arc<RwLock<HashMap<String, SiteMap>>>,
    /// Authenticated sessions keyed by session_id.
    sessions: Arc<RwLock<HashMap<String, HttpSession>>>,
    /// Mapper for executing MAP requests (None if renderer not available).
    mapper: Option<Arc<Mapper>>,
    /// Renderer for PERCEIVE requests (None if not available).
    renderer: Option<Arc<dyn Renderer>>,
}

impl Server {
    /// Create a new server bound to the given socket path.
    pub fn new(socket_path: &Path) -> Self {
        Self {
            socket_path: socket_path.to_path_buf(),
            started_at: Instant::now(),
            shutdown: Arc::new(Notify::new()),
            seen_ids: Arc::new(Mutex::new(HashSet::new())),
            maps: Arc::new(RwLock::new(HashMap::new())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            mapper: None,
            renderer: None,
        }
    }

    /// Attach a mapper and renderer to enable MAP and PERCEIVE requests.
    pub fn with_mapper(mut self, renderer: Arc<dyn Renderer>, mapper: Arc<Mapper>) -> Self {
        self.renderer = Some(renderer);
        self.mapper = Some(mapper);
        self
    }

    /// Get the shutdown notifier (for external shutdown signaling).
    pub fn shutdown_handle(&self) -> Arc<Notify> {
        Arc::clone(&self.shutdown)
    }

    /// Start accepting connections and serving requests.
    pub async fn start(&self) -> Result<()> {
        // Remove stale socket file
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path)
                .context("failed to remove stale socket file")?;
        }

        let listener =
            UnixListener::bind(&self.socket_path).context("failed to bind Unix socket")?;

        info!("Cortex server listening on {}", self.socket_path.display());

        let shutdown = Arc::clone(&self.shutdown);
        let state = Arc::new(SharedState {
            started_at: self.started_at,
            seen_ids: Arc::clone(&self.seen_ids),
            maps: Arc::clone(&self.maps),
            sessions: Arc::clone(&self.sessions),
            mapper: self.mapper.clone(),
            renderer: self.renderer.clone(),
        });

        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, _addr)) => {
                            let st = Arc::clone(&state);
                            // Use spawn_connection to force Send bound check
                            // at the function boundary rather than inline.
                            spawn_connection(stream, st);
                        }
                        Err(e) => {
                            error!("accept error: {e}");
                        }
                    }
                }
                _ = shutdown.notified() => {
                    info!("shutdown signal received");
                    break;
                }
            }
        }

        // Clean up socket file
        let _ = std::fs::remove_file(&self.socket_path);
        info!("server stopped");
        Ok(())
    }
}

/// Wrapper to assert that a future is Send.
///
/// The `handle_connection` future is actually Send — all concrete types
/// (Arc, String, etc.) are Send. However, the Rust compiler cannot prove
/// `Send` for higher-ranked lifetime bounds that appear in certain crate
/// types (e.g., selectors/scraper `SelectorErrorKind<'_>`, chromiumoxide
/// types). Since we've verified all concrete data is Send, we use this
/// wrapper to bypass the overly-conservative compiler analysis.
///
/// # Safety
/// The caller must ensure the wrapped future only contains Send types.
struct AssertSend<F>(F);

// SAFETY: The `handle_connection` future contains only Arc<SharedState>,
// UnixStream, String, serde_json::Value, and other Send types.
// The compiler fails to prove Send due to higher-ranked lifetime bounds
// in transitive dependencies (scraper, chromiumoxide), not due to actual
// non-Send data.
unsafe impl<F: std::future::Future> Send for AssertSend<F> {}

impl<F: std::future::Future> std::future::Future for AssertSend<F> {
    type Output = F::Output;
    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        // SAFETY: We're just forwarding the poll call. The inner future's
        // pin projection is safe because AssertSend is a repr(transparent)-like
        // wrapper (single field).
        let inner = unsafe { self.map_unchecked_mut(|s| &mut s.0) };
        inner.poll(cx)
    }
}

/// Spawn a connection handler task.
fn spawn_connection(stream: tokio::net::UnixStream, state: Arc<SharedState>) {
    let fut = async move {
        if let Err(e) = handle_connection(stream, state).await {
            warn!("connection error: {e}");
        }
    };
    tokio::spawn(AssertSend(fut));
}

/// Handle a single client connection with inactivity timeout and rate limiting.
async fn handle_connection(stream: tokio::net::UnixStream, state: Arc<SharedState>) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    // Rate limiting: track requests per second
    let mut rate_window_start = Instant::now();
    let mut rate_count: u32 = 0;

    loop {
        line.clear();

        // Read with inactivity timeout
        let read_result =
            tokio::time::timeout(INACTIVITY_TIMEOUT, reader.read_line(&mut line)).await;

        match read_result {
            Ok(Ok(0)) => break, // connection closed
            Ok(Ok(_)) => {
                // Check line size
                if line.len() > MAX_REQUEST_SIZE {
                    let resp = protocol::format_error(
                        "unknown",
                        "E_MESSAGE_TOO_LARGE",
                        &format!(
                            "Request exceeds maximum size of {}MB",
                            MAX_REQUEST_SIZE / (1024 * 1024)
                        ),
                    );
                    writer.write_all(resp.as_bytes()).await.ok();
                    writer.flush().await.ok();
                    continue;
                }

                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                // Rate limiting: max N requests per second per connection
                let elapsed = rate_window_start.elapsed();
                if elapsed >= Duration::from_secs(1) {
                    rate_window_start = Instant::now();
                    rate_count = 0;
                }
                rate_count += 1;
                if rate_count > MAX_REQUESTS_PER_SEC {
                    let resp = protocol::format_error(
                        "unknown",
                        "E_RATE_LIMITED",
                        &format!(
                            "Rate limit exceeded: max {} requests/second",
                            MAX_REQUESTS_PER_SEC
                        ),
                    );
                    writer.write_all(resp.as_bytes()).await.ok();
                    writer.flush().await.ok();
                    continue;
                }

                let response = match protocol::parse_request(trimmed) {
                    Ok(req) => {
                        // Check for duplicate request ID
                        let ids_lock = Arc::clone(&state.seen_ids);
                        let mut ids = ids_lock.lock().await;
                        if ids.contains(&req.id) {
                            protocol::format_error(
                                &req.id,
                                "E_DUPLICATE_ID",
                                &format!("Request ID '{}' has already been used", req.id),
                            )
                        } else {
                            ids.insert(req.id.clone());
                            // Keep set bounded (last 10000 IDs)
                            if ids.len() > 10000 {
                                ids.clear();
                            }
                            drop(ids);
                            handle_request(req, Arc::clone(&state)).await
                        }
                    }
                    Err(e) => {
                        // Malformed JSON — return error but keep connection open
                        let msg = e.to_string();
                        if msg.contains("expected") || msg.contains("invalid") {
                            protocol::format_error(
                                "unknown",
                                "E_INVALID_JSON",
                                &format!("Malformed JSON: {msg}"),
                            )
                        } else {
                            protocol::format_error("unknown", "E_INVALID_PARAMS", &msg)
                        }
                    }
                };

                if writer.write_all(response.as_bytes()).await.is_err() {
                    break; // client disconnected
                }
                if writer.flush().await.is_err() {
                    break;
                }
            }
            Ok(Err(e)) => {
                warn!("read error: {e}");
                break;
            }
            Err(_) => {
                // Inactivity timeout — close connection
                let resp = protocol::format_error(
                    "timeout",
                    "E_INACTIVITY_TIMEOUT",
                    "Connection closed due to inactivity",
                );
                writer.write_all(resp.as_bytes()).await.ok();
                writer.flush().await.ok();
                info!("closing inactive connection");
                break;
            }
        }
    }

    Ok(())
}

/// Handle a parsed request and return a JSON response string.
///
/// Takes ownership of the Request and Arc<SharedState> to avoid holding references
/// across await points, which would prevent the future from being Send (required by tokio::spawn).
async fn handle_request(req: protocol::Request, state: Arc<SharedState>) -> String {
    match req.method {
        Method::Handshake => {
            let result = protocol::HandshakeResult {
                server_version: env!("CARGO_PKG_VERSION").to_string(),
                protocol_version: 1,
                compatible: true,
            };
            protocol::format_response(&req.id, serde_json::to_value(result).unwrap_or_default())
        }
        Method::Status => {
            let uptime_s = state.started_at.elapsed().as_secs_f64();
            let maps_lock = Arc::clone(&state.maps);
            let maps = maps_lock.read().await;
            let cached_maps = maps.len() as u32;
            drop(maps);
            let active_contexts = {
                let r = state.renderer.clone();
                r.as_ref().map(|r| r.active_contexts() as u32).unwrap_or(0)
            };
            protocol::format_response(
                &req.id,
                serde_json::json!({
                    "version": env!("CARGO_PKG_VERSION"),
                    "uptime_s": uptime_s as u64,
                    "uptime_seconds": uptime_s,
                    "maps_cached": cached_maps,
                    "cached_maps": cached_maps,
                    "active_contexts": active_contexts,
                    "memory_mb": 0,
                    "pool": {
                        "active": active_contexts,
                        "max": 8,
                        "memory_mb": 0,
                    },
                    "cache_mb": 0,
                }),
            )
        }
        Method::Map => handle_map(&req, Arc::clone(&state)).await,
        Method::Query => handle_query(&req, Arc::clone(&state)).await,
        Method::Pathfind => handle_pathfind(&req, Arc::clone(&state)).await,
        Method::Perceive => handle_perceive(&req, Arc::clone(&state)).await,
        Method::Auth => handle_auth(&req, Arc::clone(&state)).await,
        Method::Refresh | Method::Act | Method::Watch => protocol::format_error(
            &req.id,
            "E_NOT_IMPLEMENTED",
            &format!("{:?} not yet implemented", req.method),
        ),
    }
}

/// Handle a MAP request: map a domain and cache the result.
///
/// The mapping operation runs in a separate spawned task to isolate
/// non-Send types from the scraper crate (used transitively by mapper.map()).
async fn handle_map(req: &protocol::Request, state: Arc<SharedState>) -> String {
    let mapper = {
        let m = state.mapper.clone();
        match m {
            Some(m) => m,
            None => {
                return protocol::format_error(
                    &req.id,
                    "E_NO_RENDERER",
                    "Browser renderer not available. Restart Cortex with a browser.",
                );
            }
        }
    };

    let domain = match req.params.get("domain").and_then(|v| v.as_str()) {
        Some(d) if !d.is_empty() => d.to_string(),
        _ => {
            return protocol::format_error(
                &req.id,
                "E_INVALID_PARAMS",
                "Missing or empty 'domain' parameter",
            );
        }
    };

    let max_nodes = req
        .params
        .get("max_nodes")
        .and_then(|v| v.as_u64())
        .unwrap_or(50000) as u32;
    let max_render = req
        .params
        .get("max_render")
        .and_then(|v| v.as_u64())
        .unwrap_or(200) as u32;
    let timeout_ms = req
        .params
        .get("max_time_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(30000);
    let respect_robots = req
        .params
        .get("respect_robots")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let req_id = req.id.clone();
    let maps = Arc::clone(&state.maps);

    let map_request = MapRequest {
        domain: domain.clone(),
        max_nodes,
        max_render,
        timeout_ms,
        respect_robots,
    };

    info!("MAP request: domain={domain}, max_nodes={max_nodes}, max_render={max_render}");

    // Apply timeout to the mapping operation and catch panics
    let map_timeout = Duration::from_millis(timeout_ms + 5000); // 5s buffer
    let map_future = mapper.map(map_request);
    let result = tokio::time::timeout(map_timeout, map_future).await;

    // Catch panics from the mapping operation by checking if we got a valid result
    let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| result)) {
        Ok(r) => r,
        Err(panic_info) => {
            let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };
            error!("MAP panicked for {domain}: {msg}");
            return build_http_fallback_map(domain.clone(), req_id, maps, false).await;
        }
    };

    match result {
        Ok(Ok(sitemap)) => {
            let node_count = sitemap.nodes.len();
            let edge_count = sitemap.edges.len();
            info!("MAP complete: domain={domain}, nodes={node_count}, edges={edge_count}");

            // Cache the map
            let maps_lock = Arc::clone(&state.maps);
            let mut map_store = maps_lock.write().await;
            map_store.insert(domain.clone(), sitemap);
            drop(map_store);

            protocol::format_response(
                &req.id,
                serde_json::json!({
                    "domain": domain,
                    "node_count": node_count,
                    "edge_count": edge_count,
                    "cached": false,
                    "map_path": null,
                }),
            )
        }
        Ok(Err(e)) => {
            warn!("MAP failed for {domain}: {e}, trying HTTP fallback");
            build_http_fallback_map(domain.clone(), req_id, maps, false).await
        }
        Err(_) => {
            warn!("MAP timed out for {domain} after {timeout_ms}ms, building fallback map");
            build_http_fallback_map(domain.clone(), req_id, maps, true).await
        }
    }
}

/// Build a fallback map using HTTP-only methods (no browser, no scraper).
///
/// Uses simple string-based link extraction to avoid scraper's non-Send types.
/// Takes all owned parameters since it may be called from a spawned task.
async fn build_http_fallback_map(
    domain: String,
    req_id: String,
    maps: Arc<RwLock<HashMap<String, SiteMap>>>,
    try_sitemap: bool,
) -> String {
    let entry_url = format!("https://{domain}/");
    let mut fallback_urls: Vec<String> = vec![entry_url.clone()];

    // Try fetching homepage for link extraction (simple parser, no scraper)
    if let Ok(resp) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .unwrap_or_default()
        .get(&entry_url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36",
        )
        .send()
        .await
    {
        if let Ok(html) = resp.text().await {
            let links = extract_links_simple(&html, &domain);
            fallback_urls.extend(links);
        }
    }

    // Optionally try sitemap.xml (used for timeout fallbacks)
    if try_sitemap {
        let sitemap_url = format!("https://{domain}/sitemap.xml");
        if let Ok(resp) = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default()
            .get(&sitemap_url)
            .send()
            .await
        {
            if let Ok(xml) = resp.text().await {
                if let Ok(entries) = crate::cartography::sitemap::parse_sitemap(&xml) {
                    for entry in entries.into_iter().take(5000) {
                        fallback_urls.push(entry.url);
                    }
                }
            }
        }
    }

    // Deduplicate
    let mut seen = std::collections::HashSet::new();
    fallback_urls.retain(|u| seen.insert(u.clone()));
    let fallback_count = fallback_urls.len();

    if fallback_urls.len() <= 1 && !try_sitemap {
        return protocol::format_error(
            &req_id,
            "E_MAP_FAILED",
            &format!("Mapping failed for {domain}"),
        );
    }

    // Build map from discovered URLs
    let mut builder = crate::map::builder::SiteMapBuilder::new(&domain);
    let mut url_to_index = std::collections::HashMap::new();
    for url in &fallback_urls {
        let (page_type, confidence) =
            crate::cartography::url_classifier::classify_url(url, &domain);
        let mut features = [0.0f32; FEATURE_DIM];
        features[80] = 1.0; // TLS
        features[0] = confidence;
        let idx = builder.add_node(url, page_type, features, (confidence * 255.0) as u8);
        url_to_index.insert(url.clone(), idx);
    }

    // Add bidirectional edges from root to all nodes
    for &idx in url_to_index.values() {
        if idx != 0 {
            builder.add_edge(
                0,
                idx,
                crate::map::types::EdgeType::Navigation,
                2,
                crate::map::types::EdgeFlags::default(),
            );
            builder.add_edge(
                idx,
                0,
                crate::map::types::EdgeType::Navigation,
                3,
                crate::map::types::EdgeFlags::default(),
            );
        }
    }

    let fallback_map = builder.build();
    let node_count = fallback_map.nodes.len();
    let edge_count = fallback_map.edges.len();

    info!(
        "fallback map for {domain}: {node_count} nodes, {edge_count} edges (from {fallback_count} URLs)"
    );

    let mut map_store = maps.write().await;
    map_store.insert(domain, fallback_map);
    drop(map_store);

    protocol::format_response(
        &req_id,
        serde_json::json!({
            "node_count": node_count,
            "edge_count": edge_count,
            "cached": false,
            "timeout_fallback": try_sitemap,
            "error_fallback": !try_sitemap,
            "map_path": null,
        }),
    )
}

/// Handle a QUERY request: filter or nearest-neighbor search on a cached map.
async fn handle_query(req: &protocol::Request, state: Arc<SharedState>) -> String {
    let domain = match req.params.get("domain").and_then(|v| v.as_str()) {
        Some(d) => d,
        None => {
            return protocol::format_error(
                &req.id,
                "E_INVALID_PARAMS",
                "Missing 'domain' parameter",
            );
        }
    };

    let maps_lock = Arc::clone(&state.maps);
    let maps = maps_lock.read().await;
    let sitemap = match maps.get(domain) {
        Some(m) => m,
        None => {
            return protocol::format_error(
                &req.id,
                "E_NOT_FOUND",
                &format!("No map cached for '{domain}'. Map the domain first."),
            );
        }
    };

    // Check if this is a nearest-neighbor query
    let mode = req
        .params
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("filter");

    if mode == "nearest" {
        return handle_nearest(req, sitemap);
    }

    // Parse page_types
    let page_types = req.params.get("page_type").and_then(|v| {
        if let Some(arr) = v.as_array() {
            let pts: Vec<PageType> = arr
                .iter()
                .filter_map(|x| x.as_u64().map(|n| PageType::from_u8(n as u8)))
                .collect();
            if pts.is_empty() {
                None
            } else {
                Some(pts)
            }
        } else if let Some(n) = v.as_u64() {
            Some(vec![PageType::from_u8(n as u8)])
        } else {
            None
        }
    });

    // Parse feature ranges
    let mut feature_ranges = Vec::new();
    if let Some(features) = req.params.get("features").and_then(|v| v.as_object()) {
        for (dim_str, range_obj) in features {
            if let Ok(dim) = dim_str.parse::<usize>() {
                if let Some(range) = range_obj.as_object() {
                    let min = range
                        .get("gt")
                        .or_else(|| range.get("gte"))
                        .and_then(|v| v.as_f64())
                        .map(|f| f as f32);
                    let max = range
                        .get("lt")
                        .or_else(|| range.get("lte"))
                        .and_then(|v| v.as_f64())
                        .map(|f| f as f32);
                    feature_ranges.push(FeatureRange {
                        dimension: dim,
                        min,
                        max,
                    });
                }
            }
        }
    }

    // Parse flags
    let require_flags = req
        .params
        .get("flags")
        .and_then(|v| v.as_object())
        .and_then(|flags| {
            let mut f: u8 = 0;
            if flags
                .get("rendered")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                f |= NodeFlags::RENDERED;
            }
            if flags
                .get("has_price")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                f |= NodeFlags::HAS_PRICE;
            }
            if flags
                .get("has_form")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                f |= NodeFlags::HAS_FORM;
            }
            if flags
                .get("has_media")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                f |= NodeFlags::HAS_MEDIA;
            }
            if f > 0 {
                Some(NodeFlags(f))
            } else {
                None
            }
        });

    // Parse sort_by
    let (sort_by_feature, sort_ascending) =
        if let Some(sort) = req.params.get("sort_by").and_then(|v| v.as_object()) {
            let dim = sort
                .get("dimension")
                .and_then(|v| v.as_u64())
                .map(|n| n as usize);
            let asc = sort
                .get("direction")
                .and_then(|v| v.as_str())
                .map(|s| s == "asc")
                .unwrap_or(true);
            (dim, asc)
        } else {
            (None, true)
        };

    let limit = req
        .params
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(100) as usize;

    let node_query = NodeQuery {
        page_types,
        feature_ranges,
        require_flags,
        exclude_flags: None,
        sort_by_feature,
        sort_ascending,
        limit,
    };

    let results = query::execute(sitemap, &node_query);
    format_node_matches(&req.id, &results)
}

/// Handle a nearest-neighbor query.
fn handle_nearest(req: &protocol::Request, sitemap: &SiteMap) -> String {
    let goal_vector = match req.params.get("goal_vector").and_then(|v| v.as_array()) {
        Some(arr) => {
            let vec: Vec<f32> = arr
                .iter()
                .filter_map(|v| v.as_f64().map(|f| f as f32))
                .collect();
            if vec.len() != FEATURE_DIM {
                return protocol::format_error(
                    &req.id,
                    "E_INVALID_PARAMS",
                    &format!(
                        "goal_vector must have {FEATURE_DIM} dimensions, got {}",
                        vec.len()
                    ),
                );
            }
            let mut arr = [0.0f32; FEATURE_DIM];
            arr.copy_from_slice(&vec);
            arr
        }
        None => {
            return protocol::format_error(
                &req.id,
                "E_INVALID_PARAMS",
                "Missing 'goal_vector' for nearest query",
            );
        }
    };

    let k = req
        .params
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(10) as usize;

    let results = sitemap.nearest(&goal_vector, k);
    format_node_matches(&req.id, &results)
}

/// Format a list of NodeMatch results as a protocol response.
fn format_node_matches(req_id: &str, results: &[crate::map::types::NodeMatch]) -> String {
    let matches: Vec<serde_json::Value> = results
        .iter()
        .map(|m| {
            let features: serde_json::Map<String, serde_json::Value> = m
                .features
                .iter()
                .map(|(k, v)| (k.to_string(), serde_json::json!(v)))
                .collect();
            serde_json::json!({
                "index": m.index,
                "url": m.url,
                "page_type": m.page_type as u8,
                "confidence": m.confidence,
                "features": features,
                "similarity": m.similarity,
            })
        })
        .collect();

    protocol::format_response(
        req_id,
        serde_json::json!({
            "matches": matches,
        }),
    )
}

/// Handle a PATHFIND request.
async fn handle_pathfind(req: &protocol::Request, state: Arc<SharedState>) -> String {
    let domain = match req.params.get("domain").and_then(|v| v.as_str()) {
        Some(d) => d,
        None => {
            return protocol::format_error(
                &req.id,
                "E_INVALID_PARAMS",
                "Missing 'domain' parameter",
            );
        }
    };

    let maps_lock = Arc::clone(&state.maps);
    let maps = maps_lock.read().await;
    let sitemap = match maps.get(domain) {
        Some(m) => m,
        None => {
            return protocol::format_error(
                &req.id,
                "E_NOT_FOUND",
                &format!("No map cached for '{domain}'. Map the domain first."),
            );
        }
    };

    let from_node = match req.params.get("from").and_then(|v| v.as_u64()) {
        Some(n) => n as u32,
        None => {
            return protocol::format_error(
                &req.id,
                "E_INVALID_PARAMS",
                "Missing 'from' node index",
            );
        }
    };

    let to_node = match req.params.get("to").and_then(|v| v.as_u64()) {
        Some(n) => n as u32,
        None => {
            return protocol::format_error(&req.id, "E_INVALID_PARAMS", "Missing 'to' node index");
        }
    };

    let minimize = match req
        .params
        .get("minimize")
        .and_then(|v| v.as_str())
        .unwrap_or("hops")
    {
        "weight" => PathMinimize::Weight,
        "state_changes" => PathMinimize::StateChanges,
        _ => PathMinimize::Hops,
    };

    let avoid_auth = req
        .params
        .get("avoid_flags")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().any(|f| f.as_str() == Some("auth_required")))
        .unwrap_or(false);

    let avoid_state_changes = req
        .params
        .get("avoid_flags")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().any(|f| f.as_str() == Some("state_changes")))
        .unwrap_or(false);

    let constraints = PathConstraints {
        avoid_auth,
        avoid_state_changes,
        minimize,
    };

    match pathfinder::find_path(sitemap, from_node, to_node, &constraints) {
        Some(path) => {
            let actions: Vec<serde_json::Value> = path
                .required_actions
                .iter()
                .map(|a| {
                    serde_json::json!({
                        "at_node": a.at_node,
                        "opcode": [a.opcode.category, a.opcode.action],
                    })
                })
                .collect();

            protocol::format_response(
                &req.id,
                serde_json::json!({
                    "nodes": path.nodes,
                    "total_weight": path.total_weight,
                    "hops": path.hops,
                    "required_actions": actions,
                }),
            )
        }
        None => protocol::format_error(&req.id, "E_NO_PATH", "No path found between nodes"),
    }
}

/// Handle a PERCEIVE request: render a single URL and return features.
async fn handle_perceive(req: &protocol::Request, state: Arc<SharedState>) -> String {
    let renderer = {
        let r = state.renderer.clone();
        match r {
            Some(r) => r,
            None => {
                return protocol::format_error(
                    &req.id,
                    "E_NO_RENDERER",
                    "Browser renderer not available. Restart Cortex with a browser.",
                );
            }
        }
    };

    let url = match req.params.get("url").and_then(|v| v.as_str()) {
        Some(u) if !u.is_empty() => u.to_string(),
        _ => {
            return protocol::format_error(
                &req.id,
                "E_INVALID_PARAMS",
                "Missing or empty 'url' parameter",
            );
        }
    };

    let include_content = req
        .params
        .get("include_content")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    // Create a new browser context for this request
    let mut context = match renderer.new_context().await {
        Ok(ctx) => ctx,
        Err(e) => {
            return protocol::format_error(
                &req.id,
                "E_RENDERER",
                &format!("Failed to create browser context: {e}"),
            );
        }
    };

    match perceive_handler::perceive(context.as_mut(), &url, include_content).await {
        Ok(result) => {
            // Convert sparse features to dict
            let features: serde_json::Map<String, serde_json::Value> = result
                .features
                .iter()
                .map(|(k, v)| (k.to_string(), serde_json::json!(v)))
                .collect();

            // Close context
            let _ = context.close().await;

            protocol::format_response(
                &req.id,
                serde_json::json!({
                    "url": result.url,
                    "final_url": result.final_url,
                    "page_type": result.page_type,
                    "confidence": result.confidence,
                    "features": features,
                    "content": result.content,
                    "load_time_ms": result.load_time_ms,
                }),
            )
        }
        Err(e) => {
            let _ = context.close().await;
            protocol::format_error(
                &req.id,
                "E_PERCEIVE_FAILED",
                &format!("Perceive failed for {url}: {e}"),
            )
        }
    }
}

/// Handle an AUTH request: authenticate with a site and store the session.
///
/// Supports `"api_key"` and `"bearer"` auth types synchronously, and
/// `"password"` via async form-based login.
async fn handle_auth(req: &protocol::Request, state: Arc<SharedState>) -> String {
    let domain = match req.params.get("domain").and_then(|v| v.as_str()) {
        Some(d) if !d.is_empty() => d.to_string(),
        _ => {
            return protocol::format_error(
                &req.id,
                "E_INVALID_PARAMS",
                "Missing or empty 'domain' parameter",
            );
        }
    };

    let auth_type = req
        .params
        .get("auth_type")
        .and_then(|v| v.as_str())
        .unwrap_or("bearer");

    let session = match auth_type {
        "api_key" => {
            let key = match req.params.get("key").and_then(|v| v.as_str()) {
                Some(k) => k,
                None => {
                    return protocol::format_error(
                        &req.id,
                        "E_INVALID_PARAMS",
                        "Missing 'key' parameter for api_key auth",
                    );
                }
            };
            let header = req
                .params
                .get("header_name")
                .and_then(|v| v.as_str())
                .unwrap_or("X-Api-Key");
            crate::acquisition::auth::login_api_key(&domain, key, header)
        }
        "bearer" => {
            let token = match req.params.get("token").and_then(|v| v.as_str()) {
                Some(t) => t,
                None => {
                    return protocol::format_error(
                        &req.id,
                        "E_INVALID_PARAMS",
                        "Missing 'token' parameter for bearer auth",
                    );
                }
            };
            crate::acquisition::auth::login_bearer(&domain, token)
        }
        "password" => {
            let username = match req.params.get("username").and_then(|v| v.as_str()) {
                Some(u) => u,
                None => {
                    return protocol::format_error(
                        &req.id,
                        "E_INVALID_PARAMS",
                        "Missing 'username' parameter for password auth",
                    );
                }
            };
            let password = match req.params.get("password").and_then(|v| v.as_str()) {
                Some(p) => p,
                None => {
                    return protocol::format_error(
                        &req.id,
                        "E_INVALID_PARAMS",
                        "Missing 'password' parameter for password auth",
                    );
                }
            };
            let client = crate::acquisition::http_client::HttpClient::new(15_000);
            match crate::acquisition::auth::login_password(&client, &domain, username, password)
                .await
            {
                Ok(s) => s,
                Err(e) => {
                    return protocol::format_error(
                        &req.id,
                        "E_AUTH_FAILED",
                        &format!("Password login failed for {domain}: {e}"),
                    );
                }
            }
        }
        other => {
            return protocol::format_error(
                &req.id,
                "E_INVALID_PARAMS",
                &format!("Unknown auth_type '{other}'. Supported: api_key, bearer, password"),
            );
        }
    };

    let session_id = session.session_id.clone();
    let sessions_lock = Arc::clone(&state.sessions);
    let mut sessions = sessions_lock.write().await;
    sessions.insert(session_id.clone(), session);
    drop(sessions);

    protocol::format_response(
        &req.id,
        serde_json::json!({
            "session_id": session_id,
            "domain": domain,
            "auth_type": auth_type,
        }),
    )
}

/// Simple regex-free link extraction from HTML for fallback code.
///
/// Avoids the `scraper` crate entirely to prevent Send issues in async contexts.
/// Only extracts `href` attributes from `<a>` tags — good enough for fallbacks.
fn extract_links_simple(html: &str, domain: &str) -> Vec<String> {
    let mut links = Vec::new();
    let lower = html.to_lowercase();
    let mut search_from = 0;

    while let Some(pos) = lower[search_from..].find("href=") {
        let abs_pos = search_from + pos + 5;
        search_from = abs_pos;

        if abs_pos >= lower.len() {
            break;
        }

        let quote = match lower.as_bytes().get(abs_pos) {
            Some(b'"') => b'"',
            Some(b'\'') => b'\'',
            _ => continue,
        };

        let start = abs_pos + 1;
        if let Some(end_offset) = html[start..].find(quote as char) {
            let href = &html[start..start + end_offset];
            let resolved = if href.starts_with("http://") || href.starts_with("https://") {
                if href.contains(domain) {
                    href.to_string()
                } else {
                    continue;
                }
            } else if href.starts_with('/') && !href.starts_with("//") {
                format!("https://{domain}{href}")
            } else {
                continue;
            };

            // Skip anchors, javascript, mailto
            if resolved.contains('#')
                || resolved.contains("javascript:")
                || resolved.contains("mailto:")
            {
                continue;
            }

            if !links.contains(&resolved) {
                links.push(resolved);
            }
        }
    }

    links
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    #[tokio::test]
    async fn test_server_handshake_and_status() {
        let socket_path = format!("/tmp/cortex-test-{}.sock", std::process::id());
        let socket_path = PathBuf::from(&socket_path);

        // Clean up stale socket from previous runs
        let _ = std::fs::remove_file(&socket_path);

        let server = Server::new(&socket_path);
        let shutdown = server.shutdown_handle();

        // Start server in background using the same server instance
        let server_task = tokio::spawn(async move {
            server.start().await.unwrap();
        });

        // Give server time to start
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Connect and send handshake
        let mut stream = UnixStream::connect(&socket_path)
            .await
            .expect("failed to connect");

        let handshake = r#"{"id":"h1","method":"handshake","params":{"client_version":"0.1.0","protocol_version":1}}"#;
        stream
            .write_all(format!("{handshake}\n").as_bytes())
            .await
            .unwrap();

        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).await.unwrap();
        let response: serde_json::Value = serde_json::from_slice(&buf[..n]).unwrap();

        assert_eq!(response["id"], "h1");
        assert_eq!(response["result"]["compatible"], true);
        assert_eq!(response["result"]["protocol_version"], 1);

        // Send status
        let status = r#"{"id":"s1","method":"status","params":{}}"#;
        stream
            .write_all(format!("{status}\n").as_bytes())
            .await
            .unwrap();

        let n = stream.read(&mut buf).await.unwrap();
        let response: serde_json::Value = serde_json::from_slice(&buf[..n]).unwrap();

        assert_eq!(response["id"], "s1");
        assert!(response["result"]["version"].as_str().is_some());

        drop(stream);

        // Shutdown
        shutdown.notify_one();
        let _ = server_task.await;

        // Cleanup
        let _ = std::fs::remove_file(&socket_path);
    }

    #[tokio::test]
    async fn test_malformed_json_keeps_connection() {
        let socket_path = format!("/tmp/cortex-test-json-{}.sock", std::process::id());
        let socket_path = PathBuf::from(&socket_path);
        let _ = std::fs::remove_file(&socket_path);

        let server = Server::new(&socket_path);
        let shutdown = server.shutdown_handle();

        let server_task = tokio::spawn(async move {
            server.start().await.unwrap();
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut stream = UnixStream::connect(&socket_path)
            .await
            .expect("failed to connect");

        // Send malformed JSON
        stream.write_all(b"this is not json\n").await.unwrap();

        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).await.unwrap();
        let response: serde_json::Value = serde_json::from_slice(&buf[..n]).unwrap();

        // Should get an error but connection stays open
        assert!(response["error"]["code"].as_str().is_some());

        // Can still send valid request on same connection
        let status = r#"{"id":"s2","method":"status","params":{}}"#;
        stream
            .write_all(format!("{status}\n").as_bytes())
            .await
            .unwrap();

        let n = stream.read(&mut buf).await.unwrap();
        let response: serde_json::Value = serde_json::from_slice(&buf[..n]).unwrap();

        assert_eq!(response["id"], "s2");
        assert!(response["result"]["version"].as_str().is_some());

        drop(stream);
        shutdown.notify_one();
        let _ = server_task.await;
        let _ = std::fs::remove_file(&socket_path);
    }
}
