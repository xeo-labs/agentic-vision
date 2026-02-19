//! Tool: session_start — Start a new vision session.

use std::sync::Arc;
use tokio::sync::Mutex;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::session::VisionSessionManager;
use crate::types::{McpError, McpResult, ToolCallResult, ToolDefinition};

#[derive(Debug, Deserialize)]
struct StartParams {
    #[serde(default)]
    session_id: Option<u32>,
}

pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "session_start".to_string(),
        description: Some("Start a new vision session".to_string()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "integer", "description": "Optional explicit session ID" }
            }
        }),
    }
}

pub async fn execute(
    args: Value,
    session: &Arc<Mutex<VisionSessionManager>>,
) -> McpResult<ToolCallResult> {
    let params: StartParams =
        serde_json::from_value(args).map_err(|e| McpError::InvalidParams(e.to_string()))?;

    let mut session = session.lock().await;
    let session_id = session.start_session(params.session_id)?;

    Ok(ToolCallResult::json(&json!({
        "session_id": session_id,
        "status": "started"
    })))
}
