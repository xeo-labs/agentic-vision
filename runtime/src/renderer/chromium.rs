//! Chromium-based renderer using chromiumoxide.

use super::{NavigationResult, RenderContext, Renderer};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::page::Page;
use futures::StreamExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Find the Chromium binary path.
pub fn find_chromium() -> Option<PathBuf> {
    // 1. CORTEX_CHROMIUM_PATH env
    if let Ok(p) = std::env::var("CORTEX_CHROMIUM_PATH") {
        let path = PathBuf::from(&p);
        if path.exists() {
            return Some(path);
        }
    }

    // 2. ~/.cortex/chromium/
    if let Some(home) = dirs::home_dir() {
        let candidates = if cfg!(target_os = "macos") {
            vec![
                home.join(".cortex/chromium/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing"),
                home.join(".cortex/chromium/chrome-mac-x64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing"),
                home.join(".cortex/chromium/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing"),
                home.join(".cortex/chromium/chrome"),
            ]
        } else {
            vec![
                home.join(".cortex/chromium/chrome-linux64/chrome"),
                home.join(".cortex/chromium/chrome"),
            ]
        };
        for c in candidates {
            if c.exists() {
                return Some(c);
            }
        }
    }

    // 3. System PATH
    if let Ok(path) = which::which("google-chrome") {
        return Some(path);
    }
    if let Ok(path) = which::which("chromium") {
        return Some(path);
    }
    if let Ok(path) = which::which("chromium-browser") {
        return Some(path);
    }

    // 4. Common macOS locations
    if cfg!(target_os = "macos") {
        let common =
            PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome");
        if common.exists() {
            return Some(common);
        }
    }

    None
}

/// Chromium-based renderer.
pub struct ChromiumRenderer {
    browser: Browser,
    active_count: Arc<AtomicUsize>,
}

impl ChromiumRenderer {
    /// Create a new ChromiumRenderer, launching a headless Chromium instance.
    pub async fn new() -> Result<Self> {
        let chrome_path =
            find_chromium().context("Chromium not found. Run `cortex install`.")?;

        let config = BrowserConfig::builder()
            .chrome_executable(chrome_path)
            .arg("--headless=new")
            .arg("--disable-gpu")
            .arg("--no-sandbox")
            .arg("--disable-dev-shm-usage")
            .arg("--disable-extensions")
            .arg("--disable-background-networking")
            .build()
            .map_err(|e| anyhow::anyhow!("failed to build browser config: {e}"))?;

        let (browser, mut handler) = Browser::launch(config)
            .await
            .context("failed to launch Chromium")?;

        // Spawn the handler task
        tokio::spawn(async move {
            while let Some(event) = handler.next().await {
                let _ = event;
            }
        });

        Ok(Self {
            browser,
            active_count: Arc::new(AtomicUsize::new(0)),
        })
    }
}

#[async_trait]
impl Renderer for ChromiumRenderer {
    async fn new_context(&self) -> Result<Box<dyn RenderContext>> {
        let page = self
            .browser
            .new_page("about:blank")
            .await
            .context("failed to create new page")?;

        self.active_count.fetch_add(1, Ordering::Relaxed);

        Ok(Box::new(ChromiumContext {
            page,
            active_count: Arc::clone(&self.active_count),
        }))
    }

    async fn shutdown(&self) -> Result<()> {
        // Browser is dropped when ChromiumRenderer is dropped
        Ok(())
    }

    fn active_contexts(&self) -> usize {
        self.active_count.load(Ordering::Relaxed)
    }
}

/// A single Chromium page context.
pub struct ChromiumContext {
    page: Page,
    active_count: Arc<AtomicUsize>,
}

#[async_trait]
impl RenderContext for ChromiumContext {
    async fn navigate(&mut self, url: &str, timeout_ms: u64) -> Result<NavigationResult> {
        let start = Instant::now();

        let result = tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms),
            self.page.goto(url),
        )
        .await;

        let load_time_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(Ok(_response)) => {
                // Wait for page to be loaded
                let _ = self
                    .page
                    .wait_for_navigation()
                    .await;

                let final_url = self
                    .page
                    .url()
                    .await
                    .unwrap_or_default()
                    .map(|u| u.to_string())
                    .unwrap_or_else(|| url.to_string());

                Ok(NavigationResult {
                    final_url,
                    status: 200, // chromiumoxide doesn't easily expose status
                    redirect_chain: Vec::new(),
                    load_time_ms,
                })
            }
            Ok(Err(e)) => bail!("navigation failed: {e}"),
            Err(_) => bail!("navigation timed out after {timeout_ms}ms"),
        }
    }

    async fn execute_js(&self, script: &str) -> Result<serde_json::Value> {
        let result = self
            .page
            .evaluate(script)
            .await
            .context("JS execution failed")?;

        result
            .into_value()
            .map_err(|e| anyhow::anyhow!("failed to convert JS result: {e:?}"))
    }

    async fn get_html(&self) -> Result<String> {
        let result = self
            .page
            .evaluate("document.documentElement.outerHTML")
            .await
            .context("failed to get HTML")?;

        let html: String = result
            .into_value()
            .map_err(|e| anyhow::anyhow!("failed to convert HTML result: {e:?}"))?;

        Ok(html)
    }

    async fn get_url(&self) -> Result<String> {
        let url = self
            .page
            .url()
            .await
            .context("failed to get URL")?
            .map(|u| u.to_string())
            .unwrap_or_default();
        Ok(url)
    }

    async fn close(self: Box<Self>) -> Result<()> {
        self.active_count.fetch_sub(1, Ordering::Relaxed);
        let _ = self.page.close().await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires Chromium to be installed
    async fn test_chromium_navigate_and_execute_js() {
        let renderer = ChromiumRenderer::new()
            .await
            .expect("failed to create renderer");
        let mut ctx = renderer
            .new_context()
            .await
            .expect("failed to create context");

        // Navigate to a data URL
        let nav = ctx
            .navigate(
                "data:text/html,<h1>Hello</h1><p>World</p>",
                10000,
            )
            .await
            .expect("navigation failed");

        assert!(nav.load_time_ms < 10000);

        // Execute JS to extract heading text
        let result = ctx
            .execute_js("document.querySelector('h1').textContent")
            .await
            .expect("JS execution failed");

        assert_eq!(result.as_str().unwrap(), "Hello");

        // Get HTML
        let html = ctx.get_html().await.expect("get_html failed");
        assert!(html.contains("<h1>Hello</h1>"));
        assert!(html.contains("<p>World</p>"));

        // Close context
        ctx.close().await.expect("close failed");
        assert_eq!(renderer.active_contexts(), 0);

        renderer.shutdown().await.expect("shutdown failed");
    }
}
