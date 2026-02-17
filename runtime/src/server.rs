//! Unix domain socket server for the Cortex protocol.
//!
//! Handles connection lifecycle, inactivity timeouts, malformed JSON,
//! rate limiting, and concurrent request management.

use crate::map::types::SiteMap;
use crate::protocol::{self, Method};
use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::{Mutex, Notify, RwLock};
use tracing::{error, info, warn};

/// Inactivity timeout per connection (30 seconds).
const INACTIVITY_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum request line size (10 MB).
const MAX_REQUEST_SIZE: usize = 10 * 1024 * 1024;

/// Maximum requests per second per connection.
const MAX_REQUESTS_PER_SEC: u32 = 10;

/// The Cortex socket server.
pub struct Server {
    socket_path: PathBuf,
    started_at: Instant,
    shutdown: Arc<Notify>,
    /// Track request IDs to reject duplicates.
    seen_ids: Arc<Mutex<HashSet<String>>>,
    /// In-memory map store. RwLock allows concurrent reads (QUERY/PATHFIND)
    /// while serializing writes (MAP completion).
    maps: Arc<RwLock<HashMap<String, SiteMap>>>,
}

impl Server {
    /// Create a new server bound to the given socket path.
    pub fn new(socket_path: &Path) -> Self {
        Self {
            socket_path: socket_path.to_path_buf(),
            started_at: Instant::now(),
            shutdown: Arc::new(Notify::new()),
            seen_ids: Arc::new(Mutex::new(HashSet::new())),
            maps: Arc::new(RwLock::new(HashMap::new())),
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
        let seen_ids = Arc::clone(&self.seen_ids);

        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((stream, _addr)) => {
                            let uptime = started_at.elapsed().as_secs();
                            let ids = Arc::clone(&seen_ids);
                            tokio::spawn(async move {
                                if let Err(e) = handle_connection(stream, uptime, ids).await {
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

/// Handle a single client connection with inactivity timeout and rate limiting.
async fn handle_connection(
    stream: tokio::net::UnixStream,
    uptime_s: u64,
    seen_ids: Arc<Mutex<HashSet<String>>>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    // Rate limiting: track requests per second
    let mut rate_window_start = Instant::now();
    let mut rate_count: u32 = 0;

    loop {
        line.clear();

        // Read with inactivity timeout
        let read_result = tokio::time::timeout(
            INACTIVITY_TIMEOUT,
            reader.read_line(&mut line),
        )
        .await;

        match read_result {
            Ok(Ok(0)) => break, // connection closed
            Ok(Ok(_)) => {
                // Check line size
                if line.len() > MAX_REQUEST_SIZE {
                    let resp = protocol::format_error(
                        "unknown",
                        "E_MESSAGE_TOO_LARGE",
                        &format!(
                            "Request exceeds maximum size of {}MB",
                            MAX_REQUEST_SIZE / (1024 * 1024)
                        ),
                    );
                    writer.write_all(resp.as_bytes()).await.ok();
                    writer.flush().await.ok();
                    continue;
                }

                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                // Rate limiting: max N requests per second per connection
                let elapsed = rate_window_start.elapsed();
                if elapsed >= Duration::from_secs(1) {
                    rate_window_start = Instant::now();
                    rate_count = 0;
                }
                rate_count += 1;
                if rate_count > MAX_REQUESTS_PER_SEC {
                    let resp = protocol::format_error(
                        "unknown",
                        "E_RATE_LIMITED",
                        &format!(
                            "Rate limit exceeded: max {} requests/second",
                            MAX_REQUESTS_PER_SEC
                        ),
                    );
                    writer.write_all(resp.as_bytes()).await.ok();
                    writer.flush().await.ok();
                    continue;
                }

                let response = match protocol::parse_request(trimmed) {
                    Ok(req) => {
                        // Check for duplicate request ID
                        let mut ids = seen_ids.lock().await;
                        if ids.contains(&req.id) {
                            protocol::format_error(
                                &req.id,
                                "E_DUPLICATE_ID",
                                &format!("Request ID '{}' has already been used", req.id),
                            )
                        } else {
                            ids.insert(req.id.clone());
                            // Keep set bounded (last 10000 IDs)
                            if ids.len() > 10000 {
                                ids.clear();
                            }
                            drop(ids);
                            handle_request(&req, uptime_s)
                        }
                    }
                    Err(e) => {
                        // Malformed JSON — return error but keep connection open
                        let msg = e.to_string();
                        if msg.contains("expected") || msg.contains("invalid") {
                            protocol::format_error(
                                "unknown",
                                "E_INVALID_JSON",
                                &format!("Malformed JSON: {msg}"),
                            )
                        } else {
                            protocol::format_error(
                                "unknown",
                                "E_INVALID_PARAMS",
                                &msg,
                            )
                        }
                    }
                };

                if writer.write_all(response.as_bytes()).await.is_err() {
                    break; // client disconnected
                }
                if writer.flush().await.is_err() {
                    break;
                }
            }
            Ok(Err(e)) => {
                warn!("read error: {e}");
                break;
            }
            Err(_) => {
                // Inactivity timeout — close connection
                let resp = protocol::format_error(
                    "timeout",
                    "E_INACTIVITY_TIMEOUT",
                    "Connection closed due to 30s inactivity",
                );
                writer.write_all(resp.as_bytes()).await.ok();
                writer.flush().await.ok();
                info!("closing inactive connection");
                break;
            }
        }
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
        Method::Map => protocol::format_error(
            &req.id,
            "E_NO_MAP_CACHE",
            "Map caching not yet available; use CLI `cortex map <domain>` instead",
        ),
        Method::Query => protocol::format_error(
            &req.id,
            "E_NO_MAP_CACHE",
            "No maps cached in server; map a domain first",
        ),
        Method::Pathfind => protocol::format_error(
            &req.id,
            "E_NO_MAP_CACHE",
            "No maps cached in server; map a domain first",
        ),
        Method::Refresh | Method::Act | Method::Watch | Method::Perceive => {
            protocol::format_error(
                &req.id,
                "E_NOT_IMPLEMENTED",
                &format!("{:?} not yet implemented", req.method),
            )
        }
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

    #[tokio::test]
    async fn test_malformed_json_keeps_connection() {
        let socket_path = format!("/tmp/cortex-test-json-{}.sock", std::process::id());
        let socket_path = PathBuf::from(&socket_path);
        let _ = std::fs::remove_file(&socket_path);

        let server = Server::new(&socket_path);
        let shutdown = server.shutdown_handle();

        let server_task = tokio::spawn(async move {
            server.start().await.unwrap();
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let mut stream = UnixStream::connect(&socket_path)
            .await
            .expect("failed to connect");

        // Send malformed JSON
        stream
            .write_all(b"this is not json\n")
            .await
            .unwrap();

        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).await.unwrap();
        let response: serde_json::Value =
            serde_json::from_slice(&buf[..n]).unwrap();

        // Should get an error but connection stays open
        assert!(response["error"]["code"].as_str().is_some());

        // Can still send valid request on same connection
        let status = r#"{"id":"s2","method":"status","params":{}}"#;
        stream
            .write_all(format!("{status}\n").as_bytes())
            .await
            .unwrap();

        let n = stream.read(&mut buf).await.unwrap();
        let response: serde_json::Value =
            serde_json::from_slice(&buf[..n]).unwrap();

        assert_eq!(response["id"], "s2");
        assert!(response["result"]["version"].as_str().is_some());

        drop(stream);
        shutdown.notify_one();
        let _ = server_task.await;
        let _ = std::fs::remove_file(&socket_path);
    }
}
