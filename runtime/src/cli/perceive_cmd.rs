//! `cortex perceive <url>` — perceive a single live page.

use anyhow::Result;

/// Run the perceive command.
pub async fn run(url: &str, format: &str) -> Result<()> {
    println!("cortex perceive: not yet implemented");
    println!("  url:    {url}");
    println!("  format: {format}");
    Ok(())
}
