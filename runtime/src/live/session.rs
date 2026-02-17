//! Session management for multi-step browser flows.
//!
//! A session holds a persistent browser context with cookies,
//! allowing agents to perform multi-step workflows (e.g., login → navigate → purchase).

use crate::renderer::RenderContext;
use anyhow::{bail, Result};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Maximum absolute age for any session context (30 minutes).
/// Even active sessions are closed after this to prevent stale browser state.
const MAX_CONTEXT_AGE: Duration = Duration::from_secs(30 * 60);

/// Idle timeout before a context is considered abandoned (5 minutes).
const IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// A persistent browser session.
pub struct Session {
    /// Unique session identifier.
    pub id: String,
    /// The browser context (with cookies and state).
    context: Box<dyn RenderContext>,
    /// When the session was created.
    created_at: Instant,
    /// When the session was last accessed.
    last_accessed: Instant,
    /// Session timeout duration.
    timeout: Duration,
}

impl Session {
    /// Create a new session with a browser context.
    pub fn new(id: String, context: Box<dyn RenderContext>, timeout: Duration) -> Self {
        let now = Instant::now();
        Self {
            id,
            context,
            created_at: now,
            last_accessed: now,
            timeout,
        }
    }

    /// Check if the session has expired.
    ///
    /// A session is expired if any of:
    /// - The configured timeout has elapsed since last access
    /// - The session has been idle longer than `IDLE_TIMEOUT` (5 min)
    /// - The session's absolute age exceeds `MAX_CONTEXT_AGE` (30 min)
    pub fn is_expired(&self) -> bool {
        // Configured per-session timeout
        if self.last_accessed.elapsed() > self.timeout {
            return true;
        }
        // Hard idle limit — kill contexts idle for >5 minutes
        if self.last_accessed.elapsed() > IDLE_TIMEOUT {
            return true;
        }
        // Hard age limit — kill contexts older than 30 minutes
        if self.created_at.elapsed() > MAX_CONTEXT_AGE {
            return true;
        }
        false
    }

    /// Touch the session to update last accessed time.
    pub fn touch(&mut self) {
        self.last_accessed = Instant::now();
    }

    /// Get the browser context for this session.
    pub fn context_mut(&mut self) -> &mut dyn RenderContext {
        self.touch();
        self.context.as_mut()
    }

    /// How long the session has been alive.
    pub fn age(&self) -> Duration {
        self.created_at.elapsed()
    }

    /// Close the session and release the browser context.
    pub async fn close(self) -> Result<()> {
        self.context.close().await
    }
}

/// Manages active browser sessions.
pub struct SessionManager {
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    default_timeout: Duration,
    next_id: Arc<Mutex<u64>>,
}

impl SessionManager {
    /// Create a new session manager.
    pub fn new(default_timeout: Duration) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            default_timeout,
            next_id: Arc::new(Mutex::new(0)),
        }
    }

    /// Create a new session with a browser context.
    pub async fn create(&self, context: Box<dyn RenderContext>) -> String {
        let mut counter = self.next_id.lock().await;
        *counter += 1;
        let id = format!("sess-{}", *counter);

        let session = Session::new(id.clone(), context, self.default_timeout);
        self.sessions.lock().await.insert(id.clone(), session);

        id
    }

    /// Get a mutable reference to a session.
    pub async fn get_mut<F, R>(&self, session_id: &str, f: F) -> Result<R>
    where
        F: FnOnce(&mut Session) -> R,
    {
        let mut sessions = self.sessions.lock().await;
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| anyhow::anyhow!("session not found: {}", session_id))?;

        if session.is_expired() {
            sessions.remove(session_id);
            bail!("session expired: {}", session_id);
        }

        Ok(f(session))
    }

    /// Close and remove a session.
    pub async fn close(&self, session_id: &str) -> Result<()> {
        let session = self
            .sessions
            .lock()
            .await
            .remove(session_id)
            .ok_or_else(|| anyhow::anyhow!("session not found: {}", session_id))?;
        session.close().await
    }

    /// Remove all expired sessions.
    pub async fn cleanup_expired(&self) {
        let mut sessions = self.sessions.lock().await;
        let expired: Vec<String> = sessions
            .iter()
            .filter(|(_, s)| s.is_expired())
            .map(|(id, _)| id.clone())
            .collect();

        for id in expired {
            if let Some(session) = sessions.remove(&id) {
                let _ = session.close().await;
            }
        }
    }

    /// Number of active sessions.
    pub async fn active_count(&self) -> usize {
        self.sessions.lock().await.len()
    }
}
