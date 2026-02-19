//! Resource URI templates and static resource definitions.

use crate::types::{ResourceDefinition, ResourceTemplateDefinition};

pub fn list_templates() -> Vec<ResourceTemplateDefinition> {
    vec![
        ResourceTemplateDefinition {
            uri_template: "avis://capture/{id}".to_string(),
            name: "Visual Capture".to_string(),
            description: Some("A single visual capture with metadata and thumbnail".to_string()),
            mime_type: Some("application/json".to_string()),
        },
        ResourceTemplateDefinition {
            uri_template: "avis://session/{id}".to_string(),
            name: "Session Captures".to_string(),
            description: Some("All captures from a specific session".to_string()),
            mime_type: Some("application/json".to_string()),
        },
        ResourceTemplateDefinition {
            uri_template: "avis://timeline/{start}/{end}".to_string(),
            name: "Timeline".to_string(),
            description: Some("Captures in a timestamp range".to_string()),
            mime_type: Some("application/json".to_string()),
        },
        ResourceTemplateDefinition {
            uri_template: "avis://similar/{id}".to_string(),
            name: "Similar Captures".to_string(),
            description: Some("Top 10 visually similar captures".to_string()),
            mime_type: Some("application/json".to_string()),
        },
    ]
}

pub fn list_resources() -> Vec<ResourceDefinition> {
    vec![
        ResourceDefinition {
            uri: "avis://stats".to_string(),
            name: "Vision Statistics".to_string(),
            description: Some("Visual memory statistics".to_string()),
            mime_type: Some("application/json".to_string()),
        },
        ResourceDefinition {
            uri: "avis://recent".to_string(),
            name: "Recent Captures".to_string(),
            description: Some("Most recent 20 captures".to_string()),
            mime_type: Some("application/json".to_string()),
        },
    ]
}
