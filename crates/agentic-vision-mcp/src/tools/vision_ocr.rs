//! Tool: vision_ocr — Extract text from a capture (stub for v0.1.0).

use std::sync::Arc;
use tokio::sync::Mutex;

use serde_json::{json, Value};

use crate::session::VisionSessionManager;
use crate::types::{McpResult, ToolCallResult, ToolDefinition};

pub fn definition() -> ToolDefinition {
    ToolDefinition {
        name: "vision_ocr".to_string(),
        description: Some(
            "Extract text from a capture using OCR (requires --features ocr in v0.2.0)"
                .to_string(),
        ),
        input_schema: json!({
            "type": "object",
            "properties": {
                "capture_id": { "type": "integer" },
                "language": { "type": "string", "default": "eng" }
            },
            "required": ["capture_id"]
        }),
    }
}

pub async fn execute(
    _args: Value,
    _session: &Arc<Mutex<VisionSessionManager>>,
) -> McpResult<ToolCallResult> {
    Ok(ToolCallResult::json(&json!({
        "status": "unavailable",
        "message": "OCR is not available in v0.1.0. This feature will be added in v0.2.0 with --features ocr (Tesseract integration)."
    })))
}
