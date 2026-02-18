// Copyright 2026 Cortex Contributors
// SPDX-License-Identifier: Apache-2.0

//! Progress event types and broadcast channel for real-time mapping telemetry.
//!
//! The mapper emits `ProgressEvent`s during mapping, which flow through a
//! `tokio::sync::broadcast` channel to all subscribers (TUI, socket clients,
//! audit log). When no subscriber exists, events are silently dropped.

use serde::{Deserialize, Serialize};

/// A progress event emitted during mapping or other long operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEvent {
    /// The request ID this event belongs to.
    pub request_id: String,
    /// Monotonically increasing sequence number.
    pub seq: u64,
    /// The kind of progress event.
    pub event: ProgressEventKind,
}

/// The specific kind of progress event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ProgressEventKind {
    /// A mapping layer has started processing.
    LayerStarted {
        layer: MappingLayer,
        message: String,
    },
    /// A mapping layer completed successfully.
    LayerCompleted {
        layer: MappingLayer,
        message: String,
        duration_ms: u64,
    },
    /// A mapping layer was skipped (not needed).
    LayerSkipped { layer: MappingLayer, reason: String },
    /// Aggregate counter update during mapping (emitted periodically).
    MappingProgress {
        urls_discovered: u32,
        pages_fetched: u32,
        nodes_built: u32,
        edges_built: u32,
        active_requests: u32,
        elapsed_ms: u64,
        prices_found: u32,
        ratings_found: u32,
        actions_found: u32,
    },
    /// A single URL was fetched or processed.
    UrlProcessed {
        url: String,
        status: u16,
        page_type: Option<String>,
    },
    /// Mapping completed successfully.
    MappingComplete {
        node_count: u32,
        edge_count: u32,
        action_count: u32,
        elapsed_ms: u64,
    },
    /// A non-fatal warning occurred.
    Warning { message: String },
}

/// Identifies which mapping layer is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MappingLayer {
    /// Layer 0: robots.txt, sitemap.xml, homepage links, feeds, HEAD scan.
    L0Metadata,
    /// Layer 1: HTTP GET sample pages + structured data (JSON-LD, OpenGraph).
    L1HttpFetch,
    /// Layer 1.5: Pattern engine (CSS selectors + regex extraction).
    L15Pattern,
    /// Layer 2: API discovery for known domains.
    L2ApiDiscovery,
    /// Layer 2.5: Action discovery — HTML forms, JS endpoints, platform templates.
    L25Actions,
    /// Layer 3: Browser rendering fallback for low-completeness pages.
    L3Browser,
    /// Final graph construction from all layers.
    BuildGraph,
}

impl std::fmt::Display for MappingLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::L0Metadata => write!(f, "Metadata"),
            Self::L1HttpFetch => write!(f, "HTTP Fetch"),
            Self::L15Pattern => write!(f, "Patterns"),
            Self::L2ApiDiscovery => write!(f, "API Discovery"),
            Self::L25Actions => write!(f, "Actions"),
            Self::L3Browser => write!(f, "Browser"),
            Self::BuildGraph => write!(f, "Build Graph"),
        }
    }
}

/// Sender handle for emitting progress events.
///
/// Backed by a `tokio::sync::broadcast` channel so multiple listeners can
/// subscribe independently. When no listeners exist, `send()` returns an error
/// which we silently ignore (zero cost when nobody's watching).
pub type ProgressSender = tokio::sync::broadcast::Sender<ProgressEvent>;

/// Receiver handle for consuming progress events.
pub type ProgressReceiver = tokio::sync::broadcast::Receiver<ProgressEvent>;

/// Create a new progress broadcast channel with a bounded buffer.
///
/// Buffer size of 256 events is enough for typical mapping operations
/// (5-7 layer events + ~200 URL events + periodic counters).
pub fn channel() -> (ProgressSender, ProgressReceiver) {
    tokio::sync::broadcast::channel(256)
}

/// Convenience helper: emit a progress event, silently ignoring send errors
/// (which occur when no receivers are listening).
pub fn emit(
    tx: &Option<ProgressSender>,
    request_id: &str,
    seq: &mut u64,
    event: ProgressEventKind,
) {
    if let Some(ref sender) = tx {
        *seq += 1;
        let _ = sender.send(ProgressEvent {
            request_id: request_id.to_string(),
            seq: *seq,
            event,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_event_serialization() {
        let event = ProgressEvent {
            request_id: "test-1".to_string(),
            seq: 1,
            event: ProgressEventKind::LayerStarted {
                layer: MappingLayer::L0Metadata,
                message: "Scanning sitemap.xml".to_string(),
            },
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("L0Metadata"));
        assert!(json.contains("LayerStarted"));

        // Roundtrip
        let parsed: ProgressEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.request_id, "test-1");
        assert_eq!(parsed.seq, 1);
    }

    #[test]
    fn test_mapping_complete_serialization() {
        let event = ProgressEvent {
            request_id: "map-42".to_string(),
            seq: 10,
            event: ProgressEventKind::MappingComplete {
                node_count: 156,
                edge_count: 420,
                action_count: 47,
                elapsed_ms: 8200,
            },
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("156"));
        assert!(json.contains("MappingComplete"));
    }

    #[test]
    fn test_channel_no_receivers() {
        let (tx, rx) = channel();
        drop(rx); // No receivers
                  // Should not panic
        emit(
            &Some(tx),
            "test",
            &mut 0,
            ProgressEventKind::Warning {
                message: "test".to_string(),
            },
        );
    }

    #[test]
    fn test_emit_none_sender() {
        // Should be a no-op
        emit(
            &None,
            "test",
            &mut 0,
            ProgressEventKind::Warning {
                message: "test".to_string(),
            },
        );
    }

    #[test]
    fn test_mapping_layer_display() {
        assert_eq!(MappingLayer::L0Metadata.to_string(), "Metadata");
        assert_eq!(MappingLayer::L1HttpFetch.to_string(), "HTTP Fetch");
        assert_eq!(MappingLayer::L3Browser.to_string(), "Browser");
    }
}
