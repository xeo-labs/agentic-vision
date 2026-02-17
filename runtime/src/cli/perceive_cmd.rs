//! `cortex perceive <url>` — perceive a single live page.

use anyhow::Result;

/// Run the perceive command.
pub async fn run(url: &str, format: &str) -> Result<()> {
    println!("Perceiving {url}...");

    // TODO: When browser pool and perceive handler are wired up,
    // render the page and return its encoding here.
    println!("\nPerceive command requires a running Cortex daemon with browser pool.");
    println!("Start the daemon with: cortex start");
    println!("Output format: {format}");

    Ok(())
}
