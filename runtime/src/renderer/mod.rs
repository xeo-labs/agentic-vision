//! Renderer abstraction for browser-based page rendering.
//!
//! Defines the `Renderer` and `RenderContext` traits that abstract over
//! the browser engine (currently Chromium via chromiumoxide).

pub mod chromium;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Result of navigating to a URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavigationResult {
    /// The final URL after any redirects.
    pub final_url: String,
    /// HTTP status code.
    pub status: u16,
    /// Chain of redirect URLs.
    pub redirect_chain: Vec<String>,
    /// Time taken to load the page in milliseconds.
    pub load_time_ms: u64,
}

/// A browser engine that can create rendering contexts.
#[async_trait]
pub trait Renderer: Send + Sync {
    /// Create a new browser context (tab).
    async fn new_context(&self) -> Result<Box<dyn RenderContext>>;
    /// Shut down the browser engine.
    async fn shutdown(&self) -> Result<()>;
    /// Number of currently active contexts.
    fn active_contexts(&self) -> usize;
}

/// A single browser context (tab) for rendering pages.
#[async_trait]
pub trait RenderContext: Send + Sync {
    /// Navigate to a URL with a timeout.
    async fn navigate(&mut self, url: &str, timeout_ms: u64) -> Result<NavigationResult>;
    /// Execute JavaScript in the page context and return the result.
    async fn execute_js(&self, script: &str) -> Result<serde_json::Value>;
    /// Get the full page HTML.
    async fn get_html(&self) -> Result<String>;
    /// Get the current URL.
    async fn get_url(&self) -> Result<String>;
    /// Close this context.
    async fn close(self: Box<Self>) -> Result<()>;
}

/// A no-op renderer used when Chromium is unavailable.
///
/// The mapper's HTTP acquisition layers (0-2.5) work without a browser.
/// This stub makes Layer 3 (browser fallback) return errors, but everything
/// else still functions.
pub struct NoopRenderer;

#[async_trait]
impl Renderer for NoopRenderer {
    async fn new_context(&self) -> Result<Box<dyn RenderContext>> {
        Err(anyhow::anyhow!("Browser not available — HTTP-only mode"))
    }
    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
    fn active_contexts(&self) -> usize {
        0
    }
}
