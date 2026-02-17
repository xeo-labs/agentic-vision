//! Unix domain socket server for the Cortex protocol.

use crate::protocol::{self, Method};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::Notify;
use tracing::{error, info, warn};

/// The Cortex socket server.
pub struct Server {
    socket_path: PathBuf,
    started_at: Instant,
    shutdown: Arc<Notify>,
}

impl Server {
    /// Create a new server bound to the given socket path.
    pub fn new(socket_path: &Path) -> Self {
        Self {
            socket_path: socket_path.to_path_buf(),
            started_at: Instant::now(),
            shutdown: Arc::new(Notify::new()),
        }
    }

    /// Get the shutdown notifier (for external shutdown signaling).
    pub fn shutdown_handle(&self) -> Arc<Notify> {
        Arc::clone(&self.shutdown)
    }

    /// Start accepting connections and serving requests.
    pub async fn start(&self) -> Result<()> {
        // Remove stale socket file
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path)
                .context("failed to remove stale socket file")?;
        }

        let listener = UnixListener::bind(&self.socket_path)
            .context("failed to bind Unix socket")?;

        info!("Cortex server listening on {}", self.socket_path.display());

        let shutdown = Arc::clone(&self.shutdown);
        let started_at = self.started_at;

        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, _addr)) => {
                            let uptime = started_at.elapsed().as_secs();
                            tokio::spawn(async move {
                                if let Err(e) = handle_connection(stream, uptime).await {
                                    warn!("connection error: {e}");
                                }
                            });
                        }
                        Err(e) => {
                            error!("accept error: {e}");
                        }
                    }
                }
                _ = shutdown.notified() => {
                    info!("shutdown signal received");
                    break;
                }
            }
        }

        // Clean up socket file
        let _ = std::fs::remove_file(&self.socket_path);
        info!("server stopped");
        Ok(())
    }
}

/// Handle a single client connection.
async fn handle_connection(stream: tokio::net::UnixStream, uptime_s: u64) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        let bytes_read = reader
            .read_line(&mut line)
            .await
            .context("failed to read from socket")?;

        if bytes_read == 0 {
            break; // connection closed
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let response = match protocol::parse_request(trimmed) {
            Ok(req) => handle_request(&req, uptime_s),
            Err(e) => protocol::format_error("unknown", "E_INVALID_PARAMS", &e.to_string()),
        };

        writer
            .write_all(response.as_bytes())
            .await
            .context("failed to write response")?;
        writer.flush().await?;
    }

    Ok(())
}

/// Handle a parsed request and return a JSON response string.
fn handle_request(req: &protocol::Request, uptime_s: u64) -> String {
    match req.method {
        Method::Handshake => {
            let result = protocol::HandshakeResult {
                server_version: env!("CARGO_PKG_VERSION").to_string(),
                protocol_version: 1,
                compatible: true,
            };
            protocol::format_response(
                &req.id,
                serde_json::to_value(result).unwrap_or_default(),
            )
        }
        Method::Status => {
            let result = protocol::StatusResult {
                version: env!("CARGO_PKG_VERSION").to_string(),
                uptime_s,
                maps_cached: 0,
                pool: protocol::PoolStatus {
                    active: 0,
                    max: 8,
                    memory_mb: 0,
                },
                cache_mb: 0,
            };
            protocol::format_response(
                &req.id,
                serde_json::to_value(result).unwrap_or_default(),
            )
        }
        Method::Map
        | Method::Query
        | Method::Pathfind
        | Method::Refresh
        | Method::Act
        | Method::Watch
        | Method::Perceive => protocol::format_error(
            &req.id,
            "E_INVALID_METHOD",
            &format!("{:?} not yet implemented", req.method),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    #[tokio::test]
    async fn test_server_handshake_and_status() {
        let socket_path = format!("/tmp/cortex-test-{}.sock", std::process::id());
        let socket_path = PathBuf::from(&socket_path);

        // Clean up stale socket from previous runs
        let _ = std::fs::remove_file(&socket_path);

        let server = Server::new(&socket_path);
        let shutdown = server.shutdown_handle();

        // Start server in background using the same server instance
        let server_task = tokio::spawn(async move {
            server.start().await.unwrap();
        });

        // Give server time to start
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Connect and send handshake
        let mut stream = UnixStream::connect(&socket_path)
            .await
            .expect("failed to connect");

        let handshake = r#"{"id":"h1","method":"handshake","params":{"client_version":"0.1.0","protocol_version":1}}"#;
        stream
            .write_all(format!("{handshake}\n").as_bytes())
            .await
            .unwrap();

        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).await.unwrap();
        let response: serde_json::Value =
            serde_json::from_slice(&buf[..n]).unwrap();

        assert_eq!(response["id"], "h1");
        assert_eq!(response["result"]["compatible"], true);
        assert_eq!(response["result"]["protocol_version"], 1);

        // Send status
        let status = r#"{"id":"s1","method":"status","params":{}}"#;
        stream
            .write_all(format!("{status}\n").as_bytes())
            .await
            .unwrap();

        let n = stream.read(&mut buf).await.unwrap();
        let response: serde_json::Value =
            serde_json::from_slice(&buf[..n]).unwrap();

        assert_eq!(response["id"], "s1");
        assert!(response["result"]["version"].as_str().is_some());

        drop(stream);

        // Shutdown
        shutdown.notify_one();
        let _ = server_task.await;

        // Cleanup
        let _ = std::fs::remove_file(&socket_path);
    }
}
