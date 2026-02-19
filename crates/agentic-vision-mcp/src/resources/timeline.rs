//! Resource: avis://timeline/{start}/{end}

use std::sync::Arc;
use tokio::sync::Mutex;

use serde_json::json;

use crate::session::VisionSessionManager;
use crate::types::{McpResult, ReadResourceResult, ResourceContent};

pub async fn read_timeline(
    start: u64,
    end: u64,
    session: &Arc<Mutex<VisionSessionManager>>,
) -> McpResult<ReadResourceResult> {
    let session = session.lock().await;
    let captures = session.store().in_time_range(start, end);

    let obs_list: Vec<_> = captures
        .iter()
        .map(|o| {
            json!({
                "id": o.id,
                "timestamp": o.timestamp,
                "session_id": o.session_id,
                "dimensions": {
                    "width": o.metadata.original_width,
                    "height": o.metadata.original_height,
                },
                "labels": o.metadata.labels,
                "description": o.metadata.description,
            })
        })
        .collect();

    let content = json!({
        "start": start,
        "end": end,
        "capture_count": obs_list.len(),
        "captures": obs_list,
    });

    Ok(ReadResourceResult {
        contents: vec![ResourceContent {
            uri: format!("avis://timeline/{start}/{end}"),
            mime_type: Some("application/json".to_string()),
            text: Some(serde_json::to_string_pretty(&content).unwrap_or_default()),
            blob: None,
        }],
    })
}
