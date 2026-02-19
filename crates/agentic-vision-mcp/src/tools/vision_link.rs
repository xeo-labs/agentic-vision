//! Tool: vision_link — Link a capture to an AgenticMemory node.

use std::sync::Arc;
use tokio::sync::Mutex;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::session::VisionSessionManager;
use crate::types::{McpError, McpResult, ToolCallResult, ToolDefinition};

#[derive(Debug, Deserialize)]
struct LinkParams {
    capture_id: u64,
    memory_node_id: u64,
    #[serde(default = "default_relationship")]
    relationship: String,
}

fn default_relationship() -> String {
    "observed_during".to_string()
}

pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "vision_link".to_string(),
        description: Some("Link a visual capture to an AgenticMemory node".to_string()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "capture_id": { "type": "integer" },
                "memory_node_id": { "type": "integer" },
                "relationship": {
                    "type": "string",
                    "enum": ["observed_during", "evidence_for", "screenshot_of"],
                    "default": "observed_during"
                }
            },
            "required": ["capture_id", "memory_node_id"]
        }),
    }
}

pub async fn execute(
    args: Value,
    session: &Arc<Mutex<VisionSessionManager>>,
) -> McpResult<ToolCallResult> {
    let params: LinkParams =
        serde_json::from_value(args).map_err(|e| McpError::InvalidParams(e.to_string()))?;

    let mut session = session.lock().await;
    session.link(params.capture_id, params.memory_node_id)?;

    Ok(ToolCallResult::json(&json!({
        "capture_id": params.capture_id,
        "memory_node_id": params.memory_node_id,
        "relationship": params.relationship,
        "status": "linked"
    })))
}
