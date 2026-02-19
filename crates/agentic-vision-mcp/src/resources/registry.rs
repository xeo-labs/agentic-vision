//! Resource registration and dispatch.

use std::sync::Arc;
use tokio::sync::Mutex;

use crate::session::VisionSessionManager;
use crate::types::{
    McpError, McpResult, ReadResourceResult, ResourceDefinition, ResourceTemplateDefinition,
};

use super::{capture, session, similar, stats, templates, timeline};

pub struct ResourceRegistry;

impl ResourceRegistry {
    pub fn list_templates() -> Vec<ResourceTemplateDefinition> {
        templates::list_templates()
    }

    pub fn list_resources() -> Vec<ResourceDefinition> {
        templates::list_resources()
    }

    pub async fn read(
        uri: &str,
        session: &Arc<Mutex<VisionSessionManager>>,
    ) -> McpResult<ReadResourceResult> {
        if let Some(id_str) = uri.strip_prefix("avis://capture/") {
            let id: u64 = id_str
                .parse()
                .map_err(|_| McpError::InvalidParams(format!("Invalid capture ID: {id_str}")))?;
            capture::read_capture(id, session).await
        } else if let Some(id_str) = uri.strip_prefix("avis://session/") {
            let id: u32 = id_str
                .parse()
                .map_err(|_| McpError::InvalidParams(format!("Invalid session ID: {id_str}")))?;
            session::read_session(id, session).await
        } else if let Some(rest) = uri.strip_prefix("avis://timeline/") {
            let parts: Vec<&str> = rest.split('/').collect();
            if parts.len() != 2 {
                return Err(McpError::InvalidParams(
                    "Timeline URI must be avis://timeline/{start}/{end}".to_string(),
                ));
            }
            let start: u64 = parts[0]
                .parse()
                .map_err(|_| McpError::InvalidParams("Invalid start timestamp".to_string()))?;
            let end: u64 = parts[1]
                .parse()
                .map_err(|_| McpError::InvalidParams("Invalid end timestamp".to_string()))?;
            timeline::read_timeline(start, end, session).await
        } else if let Some(id_str) = uri.strip_prefix("avis://similar/") {
            let id: u64 = id_str
                .parse()
                .map_err(|_| McpError::InvalidParams(format!("Invalid capture ID: {id_str}")))?;
            similar::read_similar(id, session).await
        } else if uri == "avis://stats" {
            stats::read_stats(session).await
        } else if uri == "avis://recent" {
            stats::read_recent(session).await
        } else {
            Err(McpError::ResourceNotFound(uri.to_string()))
        }
    }
}
