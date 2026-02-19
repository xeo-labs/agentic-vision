//! Tool: session_end — End the current vision session.

use std::sync::Arc;
use tokio::sync::Mutex;

use serde_json::{json, Value};

use crate::session::VisionSessionManager;
use crate::types::{McpResult, ToolCallResult, ToolDefinition};

pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "session_end".to_string(),
        description: Some("End the current vision session and save".to_string()),
        input_schema: json!({
            "type": "object",
            "properties": {}
        }),
    }
}

pub async fn execute(
    _args: Value,
    session: &Arc<Mutex<VisionSessionManager>>,
) -> McpResult<ToolCallResult> {
    let mut session = session.lock().await;
    let session_id = session.end_session()?;
    let count = session.store().count();

    Ok(ToolCallResult::json(&json!({
        "session_id": session_id,
        "total_captures": count,
        "status": "ended"
    })))
}
