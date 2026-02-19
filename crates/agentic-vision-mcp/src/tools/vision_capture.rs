//! Tool: vision_capture — Capture and store an image.

use std::sync::Arc;
use tokio::sync::Mutex;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::session::VisionSessionManager;
use crate::types::{McpError, McpResult, ToolCallResult, ToolDefinition};

#[derive(Debug, Deserialize)]
struct CaptureParams {
    source: SourceParam,
    #[serde(default)]
    extract_ocr: bool,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    labels: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SourceParam {
    #[serde(rename = "type")]
    source_type: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    data: Option<String>,
    #[serde(default)]
    mime: Option<String>,
}

pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "vision_capture".to_string(),
        description: Some("Capture an image and store it in visual memory".to_string()),
        input_schema: json!({
            "type": "object",
            "properties": {
                "source": {
                    "type": "object",
                    "properties": {
                        "type": {
                            "type": "string",
                            "enum": ["file", "base64", "screenshot", "clipboard"],
                            "description": "Source type"
                        },
                        "path": { "type": "string", "description": "File path (for type=file)" },
                        "data": { "type": "string", "description": "Base64 data (for type=base64)" },
                        "mime": { "type": "string", "description": "MIME type (for type=base64)" }
                    },
                    "required": ["type"]
                },
                "extract_ocr": { "type": "boolean", "default": false },
                "description": { "type": "string" },
                "labels": { "type": "array", "items": { "type": "string" } }
            },
            "required": ["source"]
        }),
    }
}

pub async fn execute(
    args: Value,
    session: &Arc<Mutex<VisionSessionManager>>,
) -> McpResult<ToolCallResult> {
    let params: CaptureParams =
        serde_json::from_value(args).map_err(|e| McpError::InvalidParams(e.to_string()))?;

    let source_data = match params.source.source_type.as_str() {
        "file" => params
            .source
            .path
            .as_deref()
            .ok_or_else(|| McpError::InvalidParams("'path' required for file source".to_string()))?,
        "base64" => params
            .source
            .data
            .as_deref()
            .ok_or_else(|| McpError::InvalidParams("'data' required for base64 source".to_string()))?,
        other => {
            return Err(McpError::InvalidParams(format!(
                "Unsupported source type: {other}. Use 'file' or 'base64'."
            )));
        }
    };

    let mut session = session.lock().await;
    let result = session.capture(
        &params.source.source_type,
        source_data,
        params.source.mime.as_deref(),
        params.labels,
        params.description,
        params.extract_ocr,
    )?;

    Ok(ToolCallResult::json(&json!({
        "capture_id": result.capture_id,
        "timestamp": result.timestamp,
        "dimensions": {
            "width": result.width,
            "height": result.height
        },
        "embedding_dims": result.embedding_dims
    })))
}
