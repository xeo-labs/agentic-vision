// Copyright 2026 Cortex Contributors
// SPDX-License-Identifier: Apache-2.0

//! Cortex Event Bus — typed events from every component.
//!
//! The EventBus is a `tokio::sync::broadcast` channel that carries
//! [`CortexEvent`] values. Any consumer — MCP server, REST SSE endpoint,
//! web dashboard, log files — can subscribe independently. When no
//! subscribers exist, events are silently dropped (zero overhead).

use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

/// Every event Cortex emits. Serialized to JSON for SSE, MCP, and socket streaming.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CortexEvent {
    // ── Mapping Events ────────────────────
    /// A new mapping operation has started.
    MapStarted { domain: String, timestamp: String },
    /// Sitemap/robots.txt discovery found URLs.
    SitemapDiscovered {
        domain: String,
        url_count: usize,
        elapsed_ms: u64,
    },
    /// HEAD scan progress (emitted every ~500 URLs for large scans).
    HeadScanProgress {
        domain: String,
        scanned: usize,
        total: usize,
        live: usize,
        dead: usize,
    },
    /// Structured data extraction complete for a batch of pages.
    StructuredDataExtracted {
        domain: String,
        pages_fetched: usize,
        jsonld_found: usize,
        opengraph_found: usize,
        patterns_used: usize,
        elapsed_ms: u64,
    },
    /// A mapping layer finished processing.
    LayerComplete {
        domain: String,
        layer: u8,
        layer_name: String,
        nodes_added: usize,
        features_filled: usize,
        elapsed_ms: u64,
    },
    /// Mapping completed successfully.
    MapComplete {
        domain: String,
        node_count: usize,
        edge_count: usize,
        page_types: usize,
        total_ms: u64,
        browser_contexts_used: usize,
        jsonld_coverage: f32,
    },
    /// Mapping failed with an error.
    MapFailed {
        domain: String,
        error: String,
        elapsed_ms: u64,
    },

    // ── Action Events ─────────────────────
    /// An action (add to cart, submit form, etc.) has started.
    ActionStarted {
        domain: String,
        node: usize,
        action_type: String,
        execution_path: String,
    },
    /// An action completed.
    ActionComplete {
        domain: String,
        node: usize,
        action_type: String,
        success: bool,
        execution_path: String,
        elapsed_ms: u64,
    },

    // ── Auth Events ───────────────────────
    /// Authentication flow started.
    AuthStarted { domain: String, method: String },
    /// Authentication flow completed.
    AuthComplete {
        domain: String,
        method: String,
        success: bool,
    },
    /// User consent is required for OAuth/SSO.
    AuthConsentRequired {
        domain: String,
        provider: String,
        scopes: Vec<String>,
    },

    // ── Query Events ──────────────────────
    /// A query/pathfind operation was executed.
    QueryExecuted {
        domain: String,
        query_type: String,
        results_count: usize,
        elapsed_us: u64,
    },

    // ── System Events ─────────────────────
    /// Cortex runtime started.
    RuntimeStarted {
        version: String,
        http_port: Option<u16>,
        socket_path: String,
    },
    /// An agent client connected.
    AgentConnected {
        agent_type: String,
        agent_name: Option<String>,
    },
    /// An agent client disconnected.
    AgentDisconnected { agent_type: String },
    /// Periodic cache status update.
    CacheStatus {
        maps_cached: usize,
        total_nodes: usize,
        memory_mb: f32,
    },
}

/// The central event bus for Cortex.
///
/// All components emit events through this bus. Consumers subscribe
/// to receive a stream of all events.
pub struct EventBus {
    sender: broadcast::Sender<CortexEvent>,
}

impl EventBus {
    /// Create a new event bus with the given buffer capacity.
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Emit an event to all subscribers. Silently ignores if no subscribers.
    pub fn emit(&self, event: CortexEvent) {
        let _ = self.sender.send(event);
    }

    /// Subscribe to receive all future events.
    pub fn subscribe(&self) -> broadcast::Receiver<CortexEvent> {
        self.sender.subscribe()
    }
}

/// Check if an event is related to a specific domain.
pub fn event_matches_domain(event: &CortexEvent, domain: &str) -> bool {
    match event {
        CortexEvent::MapStarted { domain: d, .. }
        | CortexEvent::SitemapDiscovered { domain: d, .. }
        | CortexEvent::HeadScanProgress { domain: d, .. }
        | CortexEvent::StructuredDataExtracted { domain: d, .. }
        | CortexEvent::LayerComplete { domain: d, .. }
        | CortexEvent::MapComplete { domain: d, .. }
        | CortexEvent::MapFailed { domain: d, .. }
        | CortexEvent::ActionStarted { domain: d, .. }
        | CortexEvent::ActionComplete { domain: d, .. }
        | CortexEvent::AuthStarted { domain: d, .. }
        | CortexEvent::AuthComplete { domain: d, .. }
        | CortexEvent::AuthConsentRequired { domain: d, .. }
        | CortexEvent::QueryExecuted { domain: d, .. } => d == domain,
        // System events are not domain-specific — return true so they reach all subscribers
        CortexEvent::RuntimeStarted { .. }
        | CortexEvent::AgentConnected { .. }
        | CortexEvent::AgentDisconnected { .. }
        | CortexEvent::CacheStatus { .. } => true,
    }
}

/// Get the ISO-8601 timestamp for the current time.
pub fn now_timestamp() -> String {
    // Use a simple approach without chrono dependency
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();
    // Basic ISO format: seconds since epoch (consumers can format)
    format!("{secs}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_serialization() {
        let event = CortexEvent::MapStarted {
            domain: "example.com".to_string(),
            timestamp: "1708276800".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("MapStarted"));
        assert!(json.contains("example.com"));

        // Roundtrip
        let parsed: CortexEvent = serde_json::from_str(&json).unwrap();
        match parsed {
            CortexEvent::MapStarted { domain, .. } => assert_eq!(domain, "example.com"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_map_complete_serialization() {
        let event = CortexEvent::MapComplete {
            domain: "amazon.com".to_string(),
            node_count: 47832,
            edge_count: 142891,
            page_types: 7,
            total_ms: 2900,
            browser_contexts_used: 0,
            jsonld_coverage: 0.89,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("MapComplete"));
        assert!(json.contains("47832"));
    }

    #[test]
    fn test_event_bus_emit_no_subscribers() {
        let bus = EventBus::new(16);
        // Should not panic when no subscribers
        bus.emit(CortexEvent::RuntimeStarted {
            version: "0.4.4".to_string(),
            http_port: Some(7700),
            socket_path: "/tmp/cortex.sock".to_string(),
        });
    }

    #[test]
    fn test_event_bus_subscribe_receive() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();

        bus.emit(CortexEvent::MapStarted {
            domain: "test.com".to_string(),
            timestamp: "123".to_string(),
        });

        let event = rx.try_recv().unwrap();
        match event {
            CortexEvent::MapStarted { domain, .. } => assert_eq!(domain, "test.com"),
            _ => panic!("wrong event"),
        }
    }

    #[test]
    fn test_event_matches_domain() {
        let event = CortexEvent::MapStarted {
            domain: "example.com".to_string(),
            timestamp: "123".to_string(),
        };
        assert!(event_matches_domain(&event, "example.com"));
        assert!(!event_matches_domain(&event, "other.com"));

        // System events always match
        let sys = CortexEvent::RuntimeStarted {
            version: "0.4.4".to_string(),
            http_port: None,
            socket_path: "/tmp/cortex.sock".to_string(),
        };
        assert!(event_matches_domain(&sys, "anything"));
    }
}
