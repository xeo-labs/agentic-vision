//! Resource: avis://capture/{id}

use std::sync::Arc;
use tokio::sync::Mutex;

use serde_json::json;

use crate::session::VisionSessionManager;
use crate::types::{McpError, McpResult, ReadResourceResult, ResourceContent};

pub async fn read_capture(
    id: u64,
    session: &Arc<Mutex<VisionSessionManager>>,
) -> McpResult<ReadResourceResult> {
    let session = session.lock().await;
    let obs = session
        .store()
        .get(id)
        .ok_or(McpError::CaptureNotFound(id))?;

    use base64::Engine;
    let thumb_b64 = base64::engine::general_purpose::STANDARD.encode(&obs.thumbnail);

    let content = json!({
        "id": obs.id,
        "timestamp": obs.timestamp,
        "session_id": obs.session_id,
        "source": obs.source,
        "metadata": obs.metadata,
        "memory_link": obs.memory_link,
        "thumbnail_base64": thumb_b64,
        "embedding_dims": obs.embedding.len(),
    });

    Ok(ReadResourceResult {
        contents: vec![ResourceContent {
            uri: format!("avis://capture/{id}"),
            mime_type: Some("application/json".to_string()),
            text: Some(serde_json::to_string_pretty(&content).unwrap_or_default()),
            blob: None,
        }],
    })
}
