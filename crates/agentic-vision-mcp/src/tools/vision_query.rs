//! Tool: vision_query — Search visual memory.

use std::sync::Arc;
use tokio::sync::Mutex;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::session::VisionSessionManager;
use crate::types::{McpError, McpResult, ToolCallResult, ToolDefinition};

#[derive(Debug, Deserialize)]
struct QueryParams {
    #[serde(default)]
    session_ids: Vec<u32>,
    #[serde(default)]
    after: Option<u64>,
    #[serde(default)]
    before: Option<u64>,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default = "default_max_results")]
    max_results: usize,
}

fn default_max_results() -> usize {
    20
}

pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "vision_query".to_string(),
        description: Some("Search visual memory by filters".to_string()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "session_ids": { "type": "array", "items": { "type": "integer" } },
                "after": { "type": "integer", "description": "Unix timestamp" },
                "before": { "type": "integer", "description": "Unix timestamp" },
                "labels": { "type": "array", "items": { "type": "string" } },
                "max_results": { "type": "integer", "default": 20 }
            }
        }),
    }
}

pub async fn execute(
    args: Value,
    session: &Arc<Mutex<VisionSessionManager>>,
) -> McpResult<ToolCallResult> {
    let params: QueryParams =
        serde_json::from_value(args).map_err(|e| McpError::InvalidParams(e.to_string()))?;

    let session = session.lock().await;
    let store = session.store();

    let results: Vec<Value> = store
        .observations
        .iter()
        .filter(|o| {
            if !params.session_ids.is_empty() && !params.session_ids.contains(&o.session_id) {
                return false;
            }
            if let Some(after) = params.after {
                if o.timestamp < after {
                    return false;
                }
            }
            if let Some(before) = params.before {
                if o.timestamp > before {
                    return false;
                }
            }
            if !params.labels.is_empty()
                && !params
                    .labels
                    .iter()
                    .any(|l| o.metadata.labels.contains(l))
            {
                return false;
            }
            true
        })
        .take(params.max_results)
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
                "memory_link": o.memory_link,
            })
        })
        .collect();

    Ok(ToolCallResult::json(&json!({
        "total": results.len(),
        "observations": results,
    })))
}
