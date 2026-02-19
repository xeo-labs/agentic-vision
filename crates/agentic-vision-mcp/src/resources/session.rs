//! Resource: avis://session/{id}

use std::sync::Arc;
use tokio::sync::Mutex;

use serde_json::json;

use crate::session::VisionSessionManager;
use crate::types::{McpResult, ReadResourceResult, ResourceContent};

pub async fn read_session(
    session_id: u32,
    session: &Arc<Mutex<VisionSessionManager>>,
) -> McpResult<ReadResourceResult> {
    let session = session.lock().await;
    let captures = session.store().by_session(session_id);

    let obs_list: Vec<_> = captures
        .iter()
        .map(|o| {
            json!({
                "id": o.id,
                "timestamp": o.timestamp,
                "dimensions": {
                    "width": o.metadata.original_width,
                    "height": o.metadata.original_height,
                },
                "labels": o.metadata.labels,
                "description": o.metadata.description,
                "memory_link": o.memory_link,
            })
        })
        .collect();

    let content = json!({
        "session_id": session_id,
        "capture_count": obs_list.len(),
        "captures": obs_list,
    });

    Ok(ReadResourceResult {
        contents: vec![ResourceContent {
            uri: format!("avis://session/{session_id}"),
            mime_type: Some("application/json".to_string()),
            text: Some(serde_json::to_string_pretty(&content).unwrap_or_default()),
            blob: None,
        }],
    })
}
