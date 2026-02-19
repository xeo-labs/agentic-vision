//! Tool: vision_diff — Get detailed diff between two captures.

use std::sync::Arc;
use tokio::sync::Mutex;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::session::VisionSessionManager;
use crate::types::{McpError, McpResult, ToolCallResult, ToolDefinition};

#[derive(Debug, Deserialize)]
struct DiffParams {
    id_a: u64,
    id_b: u64,
}

pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "vision_diff".to_string(),
        description: Some("Get detailed pixel-level diff between two captures".to_string()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "id_a": { "type": "integer" },
                "id_b": { "type": "integer" }
            },
            "required": ["id_a", "id_b"]
        }),
    }
}

pub async fn execute(
    args: Value,
    session: &Arc<Mutex<VisionSessionManager>>,
) -> McpResult<ToolCallResult> {
    let params: DiffParams =
        serde_json::from_value(args).map_err(|e| McpError::InvalidParams(e.to_string()))?;

    let session = session.lock().await;
    let diff = session.diff(params.id_a, params.id_b)?;

    Ok(ToolCallResult::json(&json!({
        "before_id": diff.before_id,
        "after_id": diff.after_id,
        "similarity": diff.similarity,
        "pixel_diff_ratio": diff.pixel_diff_ratio,
        "changed_regions": diff.changed_regions,
    })))
}
