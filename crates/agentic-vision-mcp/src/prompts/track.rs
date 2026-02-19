//! Prompt: track — Guide for tracking visual changes over time.

use serde_json::Value;

use crate::types::{McpError, McpResult, PromptGetResult, PromptMessage, ToolContent};

pub fn expand(args: Value) -> McpResult<PromptGetResult> {
    let target = args
        .get("target")
        .and_then(|v| v.as_str())
        .ok_or_else(|| McpError::InvalidParams("'target' argument is required".to_string()))?;

    let duration = args
        .get("duration")
        .and_then(|v| v.as_str())
        .unwrap_or("until changes are detected");

    let text = format!(
        "Track visual changes to: {target}\n\
         Duration: {duration}\n\n\
         Please:\n\
         1. Use vision_capture to get the initial state\n\
         2. Use vision_track to configure change monitoring for the region\n\
         3. Periodically capture new states with vision_capture\n\
         4. Use vision_compare to detect when changes occur\n\
         5. After tracking completes, summarize all changes observed"
    );

    Ok(PromptGetResult {
        description: Some("Guide for tracking visual changes over time".to_string()),
        messages: vec![PromptMessage {
            role: "user".to_string(),
            content: ToolContent::Text { text },
        }],
    })
}
