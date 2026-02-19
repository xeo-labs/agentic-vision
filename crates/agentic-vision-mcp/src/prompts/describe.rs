//! Prompt: describe — Guide for describing a capture in detail.

use serde_json::Value;

use crate::types::{McpError, McpResult, PromptGetResult, PromptMessage, ToolContent};

pub fn expand(args: Value) -> McpResult<PromptGetResult> {
    let capture_id = args
        .get("capture_id")
        .ok_or_else(|| McpError::InvalidParams("'capture_id' argument is required".to_string()))?;

    let text = format!(
        "Describe capture {capture_id} in detail.\n\n\
         Please:\n\
         1. Use the avis://capture/{capture_id} resource to load the capture\n\
         2. Describe what you see in detail\n\
         3. Identify key UI elements, buttons, text fields\n\
         4. Note the layout and visual hierarchy\n\
         5. Note anything that might be relevant for future reference"
    );

    Ok(PromptGetResult {
        description: Some("Guide for describing a visual capture in detail".to_string()),
        messages: vec![PromptMessage {
            role: "user".to_string(),
            content: ToolContent::Text { text },
        }],
    })
}
