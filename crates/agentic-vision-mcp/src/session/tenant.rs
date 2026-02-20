//! Multi-tenant session registry — lazy-loads per-user vision files.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::Mutex;

use super::VisionSessionManager;
use crate::types::McpResult;

/// Registry of per-user sessions for multi-tenant mode.
pub struct VisionTenantRegistry {
    data_dir: PathBuf,
    model_path: Option<String>,
    sessions: HashMap<String, Arc<Mutex<VisionSessionManager>>>,
}

impl VisionTenantRegistry {
    /// Create a new tenant registry backed by the given data directory.
    pub fn new(data_dir: &Path, model_path: Option<&str>) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
            model_path: model_path.map(|s| s.to_string()),
            sessions: HashMap::new(),
        }
    }

    /// Get or create a session for the given user ID.
    ///
    /// On first access, creates `{data_dir}/{user_id}.avis` and opens a session.
    pub fn get_or_create(&mut self, user_id: &str) -> McpResult<Arc<Mutex<VisionSessionManager>>> {
        if let Some(session) = self.sessions.get(user_id) {
            return Ok(session.clone());
        }

        // Ensure data directory exists
        std::fs::create_dir_all(&self.data_dir).map_err(|e| {
            crate::types::McpError::InternalError(format!(
                "Failed to create data dir {}: {e}",
                self.data_dir.display()
            ))
        })?;

        let vision_path = self.data_dir.join(format!("{user_id}.avis"));
        let path_str = vision_path.display().to_string();

        tracing::info!("Opening vision store for user '{user_id}': {path_str}");

        let session = VisionSessionManager::open(&path_str, self.model_path.as_deref())?;
        let session = Arc::new(Mutex::new(session));
        self.sessions.insert(user_id.to_string(), session.clone());

        Ok(session)
    }

    /// Number of active tenant sessions.
    pub fn count(&self) -> usize {
        self.sessions.len()
    }
}
