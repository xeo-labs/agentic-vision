//! Prompt: compare — Guide for comparing two visual captures.

use serde_json::Value;

use crate::types::{McpError, McpResult, PromptGetResult, PromptMessage, ToolContent};

pub fn expand(args: Value) -> McpResult<PromptGetResult> {
    let capture_a = args
        .get("capture_a")
        .ok_or_else(|| McpError::InvalidParams("'capture_a' argument is required".to_string()))?;

    let capture_b = args
        .get("capture_b")
        .ok_or_else(|| McpError::InvalidParams("'capture_b' argument is required".to_string()))?;

    let text = format!(
        "Compare these two visual captures:\n\
         - Capture A: {capture_a}\n\
         - Capture B: {capture_b}\n\n\
         Please:\n\
         1. Use vision_compare to get the similarity score\n\
         2. Use vision_diff for detailed change analysis\n\
         3. Summarize what changed between them\n\
         4. Note any significant visual differences"
    );

    Ok(PromptGetResult {
        description: Some("Guide for comparing two visual captures".to_string()),
        messages: vec![PromptMessage {
            role: "user".to_string(),
            content: ToolContent::Text { text },
        }],
    })
}
