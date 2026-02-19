//! Resource: avis://similar/{id}

use std::sync::Arc;
use tokio::sync::Mutex;

use serde_json::json;

use crate::session::VisionSessionManager;
use crate::types::{McpResult, ReadResourceResult, ResourceContent};

pub async fn read_similar(
    capture_id: u64,
    session: &Arc<Mutex<VisionSessionManager>>,
) -> McpResult<ReadResourceResult> {
    let session = session.lock().await;
    let matches = session.find_similar(capture_id, 10, 0.5)?;

    let match_list: Vec<_> = matches
        .iter()
        .map(|m| {
            json!({
                "id": m.id,
                "similarity": m.similarity,
            })
        })
        .collect();

    let content = json!({
        "capture_id": capture_id,
        "similar_count": match_list.len(),
        "matches": match_list,
    });

    Ok(ReadResourceResult {
        contents: vec![ResourceContent {
            uri: format!("avis://similar/{capture_id}"),
            mime_type: Some("application/json".to_string()),
            text: Some(serde_json::to_string_pretty(&content).unwrap_or_default()),
            blob: None,
        }],
    })
}
