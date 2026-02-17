//! Start the Cortex daemon process.

use crate::server::Server;
use anyhow::{Context, Result};
use std::path::PathBuf;
use tracing::info;

/// Default socket path.
pub const SOCKET_PATH: &str = "/tmp/cortex.sock";

/// Get the PID file path.
pub fn pid_file_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".cortex/cortex.pid")
}

/// Start the Cortex daemon: bind socket, write PID, serve requests.
pub async fn run() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("cortex=info".parse().unwrap()),
        )
        .init();

    info!("starting Cortex v{}", env!("CARGO_PKG_VERSION"));

    let socket_path = PathBuf::from(SOCKET_PATH);

    // Remove stale socket
    if socket_path.exists() {
        std::fs::remove_file(&socket_path).ok();
    }

    // Write PID file
    let pid_path = pid_file_path();
    if let Some(parent) = pid_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&pid_path, std::process::id().to_string())
        .context("failed to write PID file")?;

    // Set up SIGTERM/SIGINT handling
    let server = Server::new(&socket_path);
    let shutdown = server.shutdown_handle();

    let shutdown_signal = shutdown.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        info!("received shutdown signal");
        shutdown_signal.notify_one();
    });

    // Run server
    let result = server.start().await;

    // Clean up PID file
    let _ = std::fs::remove_file(&pid_path);

    result
}
