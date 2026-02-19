//! Prompt: observe — Guide for capturing and describing what you see.

use serde_json::Value;

use crate::types::{McpResult, PromptGetResult, PromptMessage, ToolContent};

pub fn expand(args: Value) -> McpResult<PromptGetResult> {
    let context = args
        .get("context")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let context_section = if context.is_empty() {
        String::new()
    } else {
        format!("\nContext: {context}\n")
    };

    let text = format!(
        "I need to observe and remember what I'm seeing.\n\
         {context_section}\n\
         Please:\n\
         1. Use vision_capture to take a screenshot or load the image\n\
         2. Describe what you see in detail\n\
         3. Note any text, buttons, UI elements, or important visual features\n\
         4. If relevant, use vision_link to connect this observation to our memory graph"
    );

    Ok(PromptGetResult {
        description: Some("Guide for capturing and describing visual observations".to_string()),
        messages: vec![PromptMessage {
            role: "user".to_string(),
            content: ToolContent::Text { text },
        }],
    })
}
