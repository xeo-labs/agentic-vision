// Copyright 2026 Cortex Contributors
// SPDX-License-Identifier: Apache-2.0

//! HTTP REST API for Cortex.
//!
//! Provides a REST interface alongside the Unix socket server.
//! Every REST endpoint maps 1:1 to a protocol method, using the
//! same [`SharedState`] and [`handle_request`] dispatch.

use crate::events::{self, CortexEvent};
use crate::protocol;
use crate::server::{handle_request, SharedState};
use axum::extract::{Query, State};
use axum::response::sse::{Event, Sse};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::Value;
use std::convert::Infallible;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

/// Wrapper to assert a future is Send.
///
/// Same technique as in `server.rs` — the `handle_request` future contains
/// only Send types but the compiler cannot prove it due to higher-ranked
/// lifetime bounds in transitive dependencies (scraper, chromiumoxide).
struct AssertSend<F>(F);

// SAFETY: All concrete types in handle_request are Send. See server.rs
// for the full justification.
unsafe impl<F: std::future::Future> Send for AssertSend<F> {}

impl<F: std::future::Future> std::future::Future for AssertSend<F> {
    type Output = F::Output;
    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let inner = unsafe { self.map_unchecked_mut(|s| &mut s.0) };
        inner.poll(cx)
    }
}

/// Build the axum Router with all REST endpoints.
pub fn router(state: Arc<SharedState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/health", get(health))
        .route("/dashboard", get(dashboard))
        .route("/api/v1/status", get(handle_status))
        .route("/api/v1/events", get(events_sse))
        .route("/api/v1/map", post(handle_map))
        .route("/api/v1/query", post(handle_query))
        .route("/api/v1/pathfind", post(handle_pathfind))
        .route("/api/v1/act", post(handle_act))
        .route("/api/v1/perceive", post(handle_perceive))
        .route("/api/v1/compare", post(handle_compare))
        .route("/api/v1/auth", post(handle_auth))
        .route("/api/v1/maps", get(handle_list_maps))
        .layer(cors)
        .with_state(state)
}

/// Start the REST API server on the given port.
///
/// This runs concurrently with the Unix socket server and shares the
/// same internal state. Shut down by dropping the returned future.
pub async fn start(port: u16, state: Arc<SharedState>) -> anyhow::Result<()> {
    let app = router(state);
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    tracing::info!("REST API listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────

/// Dispatch a REST request through the Cortex protocol handler.
///
/// Wraps the JSON body as a protocol request and calls the same
/// `handle_request` function used by the socket server.
async fn dispatch(method: &str, params: Value, state: Arc<SharedState>) -> Json<Value> {
    let id = format!("rest-{}", uuid_simple());
    let req_json = serde_json::json!({
        "id": id,
        "method": method,
        "params": params,
    });

    // Parse through the protocol layer (validates structure)
    let req = match protocol::parse_request(&req_json.to_string()) {
        Ok(r) => r,
        Err(e) => {
            return Json(serde_json::json!({
                "error": { "code": "E_INVALID_PARAMS", "message": e.to_string() }
            }));
        }
    };

    // Use AssertSend + spawn to satisfy axum's Send requirement.
    // handle_request is actually Send — see server.rs for justification.
    let response_str = {
        let fut = AssertSend(handle_request(req, state));
        tokio::task::spawn(fut).await.unwrap_or_else(|e| {
            serde_json::json!({
                "error": { "code": "E_INTERNAL", "message": format!("task panicked: {e}") }
            })
            .to_string()
        })
    };

    // Parse the response string back to JSON
    match serde_json::from_str::<Value>(&response_str) {
        Ok(mut v) => {
            // Strip the protocol "id" field for cleaner REST responses
            if let Some(obj) = v.as_object_mut() {
                obj.remove("id");
            }
            // Flatten: if there's a "result" key, return its contents directly
            if let Some(result) = v.get("result").cloned() {
                Json(result)
            } else {
                Json(v)
            }
        }
        Err(_) => Json(serde_json::json!({
            "error": { "code": "E_INTERNAL", "message": "Failed to parse internal response" }
        })),
    }
}

/// Simple monotonic ID generator (no external crate needed).
fn uuid_simple() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{ts:x}-{n}")
}

// ── Handlers ────────────────────────────────────────────────────

async fn health() -> Json<Value> {
    let profile = std::env::var("CORTEX_AUTONOMIC_PROFILE")
        .unwrap_or_else(|_| "desktop".to_string())
        .trim()
        .to_ascii_lowercase();
    let migration_policy = std::env::var("CORTEX_STORAGE_MIGRATION_POLICY")
        .unwrap_or_else(|_| "auto-safe".to_string())
        .trim()
        .to_ascii_lowercase();
    let ledger_dir = std::env::var("CORTEX_HEALTH_LEDGER_DIR")
        .ok()
        .or_else(|| std::env::var("AGENTRA_HEALTH_LEDGER_DIR").ok())
        .unwrap_or_else(|| "~/.agentra/health-ledger".to_string());
    Json(serde_json::json!({
        "status": "ok",
        "autonomic": {
            "profile": profile,
            "migration_policy": migration_policy,
            "health_ledger_dir": ledger_dir
        }
    }))
}

/// Serve the embedded dashboard HTML.
async fn dashboard() -> impl IntoResponse {
    Html(include_str!("dashboard.html"))
}

/// SSE query parameters.
#[derive(serde::Deserialize, Default)]
struct EventsParams {
    domain: Option<String>,
}

/// Server-Sent Events endpoint for real-time event streaming.
///
/// Subscribes to the global event bus and streams events as SSE.
/// Optionally filters by domain via `?domain=example.com`.
async fn events_sse(
    Query(params): Query<EventsParams>,
    State(state): State<Arc<SharedState>>,
) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.event_bus.subscribe();
    let domain_filter = params.domain;

    let stream = async_stream::stream! {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    // Filter by domain if specified
                    if let Some(ref domain) = domain_filter {
                        if !events::event_matches_domain(&event, domain) {
                            continue;
                        }
                    }
                    if let Ok(json) = serde_json::to_string(&event) {
                        yield Ok(Event::default().data(json));
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    // Missed some events due to slow consumer — continue
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

/// Enhanced status endpoint returning richer data for the dashboard.
async fn handle_status(State(state): State<Arc<SharedState>>) -> Json<Value> {
    let uptime_s = state.started_at.elapsed().as_secs_f64();
    let maps_lock = Arc::clone(&state.maps);
    let maps = maps_lock.read().await;

    let map_list: Vec<Value> = maps
        .iter()
        .map(|(domain, sitemap)| {
            serde_json::json!({
                "domain": domain,
                "node_count": sitemap.nodes.len(),
                "edge_count": sitemap.edges.len(),
                "action_count": sitemap.actions.len(),
            })
        })
        .collect();
    let cached_maps = maps.len();
    let total_nodes: usize = maps.values().map(|m| m.nodes.len()).sum();
    drop(maps);

    let active_contexts = state
        .renderer
        .as_ref()
        .map(|r| r.active_contexts() as u32)
        .unwrap_or(0);

    let chromium_available = state.renderer.is_some();

    Json(serde_json::json!({
        "running": true,
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_seconds": uptime_s,
        "maps_cached": cached_maps,
        "maps": map_list,
        "total_nodes": total_nodes,
        "active_contexts": active_contexts,
        "chromium_available": chromium_available,
        "memory_mb": 0,
        "pool": {
            "active": active_contexts,
            "max": 8,
            "memory_mb": 0,
        },
    }))
}

async fn handle_map(State(state): State<Arc<SharedState>>, Json(body): Json<Value>) -> Json<Value> {
    dispatch("map", body, state).await
}

async fn handle_query(
    State(state): State<Arc<SharedState>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    dispatch("query", body, state).await
}

async fn handle_pathfind(
    State(state): State<Arc<SharedState>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    dispatch("pathfind", body, state).await
}

async fn handle_act(State(state): State<Arc<SharedState>>, Json(body): Json<Value>) -> Json<Value> {
    dispatch("act", body, state).await
}

async fn handle_perceive(
    State(state): State<Arc<SharedState>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    dispatch("perceive", body, state).await
}

async fn handle_compare(
    State(state): State<Arc<SharedState>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    // Compare is a composite: map multiple domains then query across them
    // For now, treat it like a map request — the full compare logic
    // can be added when the protocol supports it natively.
    dispatch("map", body, state).await
}

async fn handle_auth(
    State(state): State<Arc<SharedState>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    dispatch("auth", body, state).await
}

async fn handle_list_maps(State(state): State<Arc<SharedState>>) -> Json<Value> {
    let maps = state.maps.read().await;
    let list: Vec<Value> = maps
        .iter()
        .map(|(domain, sitemap)| {
            serde_json::json!({
                "domain": domain,
                "node_count": sitemap.nodes.len(),
                "edge_count": sitemap.edges.len(),
            })
        })
        .collect();
    drop(maps);
    Json(serde_json::json!({ "maps": list }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uuid_simple_unique() {
        let a = uuid_simple();
        let b = uuid_simple();
        assert_ne!(a, b);
    }
}
