//! Stop the running Cortex daemon.

use anyhow::Result;

/// Stop the Cortex daemon by reading PID file and sending SIGTERM.
pub async fn run() -> Result<()> {
    println!("cortex stop: not yet implemented");
    Ok(())
}
