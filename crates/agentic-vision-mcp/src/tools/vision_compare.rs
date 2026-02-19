//! Tool: vision_compare — Compare two captures for similarity.

use std::sync::Arc;
use tokio::sync::Mutex;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::session::VisionSessionManager;
use crate::types::{McpError, McpResult, ToolCallResult, ToolDefinition};

#[derive(Debug, Deserialize)]
struct CompareParams {
    id_a: u64,
    id_b: u64,
    #[serde(default)]
    detailed: bool,
}

pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "vision_compare".to_string(),
        description: Some("Compare two captures for visual similarity".to_string()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "id_a": { "type": "integer", "description": "First capture ID" },
                "id_b": { "type": "integer", "description": "Second capture ID" },
                "detailed": { "type": "boolean", "default": false, "description": "Include detailed diff" }
            },
            "required": ["id_a", "id_b"]
        }),
    }
}

pub async fn execute(
    args: Value,
    session: &Arc<Mutex<VisionSessionManager>>,
) -> McpResult<ToolCallResult> {
    let params: CompareParams =
        serde_json::from_value(args).map_err(|e| McpError::InvalidParams(e.to_string()))?;

    let session = session.lock().await;
    let similarity = session.compare(params.id_a, params.id_b)?;
    let is_same = similarity > 0.95;

    let mut result = json!({
        "similarity": similarity,
        "is_same": is_same,
    });

    if params.detailed {
        if let Ok(diff) = session.diff(params.id_a, params.id_b) {
            result["changed_regions"] = serde_json::to_value(&diff.changed_regions)
                .unwrap_or(Value::Array(vec![]));
            result["pixel_diff_ratio"] = json!(diff.pixel_diff_ratio);
        }
    }

    Ok(ToolCallResult::json(&result))
}
