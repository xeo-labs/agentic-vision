//! Prompt registration and dispatch.

use serde_json::Value;

use crate::types::{McpError, McpResult, PromptArgument, PromptDefinition, PromptGetResult};

use super::{compare, describe, observe, track};

pub struct PromptRegistry;

impl PromptRegistry {
    pub fn list_prompts() -> Vec<PromptDefinition> {
        vec![
            PromptDefinition {
                name: "observe".to_string(),
                description: Some(
                    "Guide for capturing and describing what you see".to_string(),
                ),
                arguments: Some(vec![PromptArgument {
                    name: "context".to_string(),
                    description: Some("Optional context about what to observe".to_string()),
                    required: false,
                }]),
            },
            PromptDefinition {
                name: "compare".to_string(),
                description: Some("Guide for comparing two visual captures".to_string()),
                arguments: Some(vec![
                    PromptArgument {
                        name: "capture_a".to_string(),
                        description: Some("First capture ID".to_string()),
                        required: true,
                    },
                    PromptArgument {
                        name: "capture_b".to_string(),
                        description: Some("Second capture ID".to_string()),
                        required: true,
                    },
                ]),
            },
            PromptDefinition {
                name: "track".to_string(),
                description: Some("Guide for tracking visual changes over time".to_string()),
                arguments: Some(vec![
                    PromptArgument {
                        name: "target".to_string(),
                        description: Some("What to track".to_string()),
                        required: true,
                    },
                    PromptArgument {
                        name: "duration".to_string(),
                        description: Some("How long to track".to_string()),
                        required: false,
                    },
                ]),
            },
            PromptDefinition {
                name: "describe".to_string(),
                description: Some("Guide for describing a capture in detail".to_string()),
                arguments: Some(vec![PromptArgument {
                    name: "capture_id".to_string(),
                    description: Some("Capture ID to describe".to_string()),
                    required: true,
                }]),
            },
        ]
    }

    pub async fn get(
        name: &str,
        arguments: Option<Value>,
    ) -> McpResult<PromptGetResult> {
        let args = arguments.unwrap_or(Value::Object(serde_json::Map::new()));

        match name {
            "observe" => observe::expand(args),
            "compare" => compare::expand(args),
            "track" => track::expand(args),
            "describe" => describe::expand(args),
            _ => Err(McpError::PromptNotFound(name.to_string())),
        }
    }
}
