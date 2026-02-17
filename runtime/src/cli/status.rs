//! Show status of the running Cortex daemon.

use crate::cli::start::SOCKET_PATH;
use anyhow::{bail, Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

/// Connect to socket and display runtime status.
pub async fn run() -> Result<()> {
    let stream = match UnixStream::connect(SOCKET_PATH).await {
        Ok(s) => s,
        Err(_) => bail!("Cortex is not running (cannot connect to {SOCKET_PATH})"),
    };

    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    // Send status request
    let req = r#"{"id":"status","method":"status","params":{}}"#;
    writer
        .write_all(format!("{req}\n").as_bytes())
        .await
        .context("failed to send status request")?;
    writer.flush().await?;

    // Read response
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .await
        .context("failed to read status response")?;

    let resp: serde_json::Value =
        serde_json::from_str(line.trim()).context("invalid status response")?;

    if let Some(result) = resp.get("result") {
        println!("Cortex Status");
        println!("=============");
        if let Some(v) = result.get("version").and_then(|v| v.as_str()) {
            println!("Version:     {v}");
        }
        if let Some(u) = result.get("uptime_s").and_then(|v| v.as_u64()) {
            let hours = u / 3600;
            let mins = (u % 3600) / 60;
            let secs = u % 60;
            println!("Uptime:      {hours}h {mins}m {secs}s");
        }
        if let Some(m) = result.get("maps_cached").and_then(|v| v.as_u64()) {
            println!("Maps cached: {m}");
        }
        if let Some(pool) = result.get("pool") {
            let active = pool.get("active").and_then(|v| v.as_u64()).unwrap_or(0);
            let max = pool.get("max").and_then(|v| v.as_u64()).unwrap_or(0);
            let mem = pool.get("memory_mb").and_then(|v| v.as_u64()).unwrap_or(0);
            println!("Pool:        {active}/{max} contexts, {mem}MB");
        }
        if let Some(c) = result.get("cache_mb").and_then(|v| v.as_u64()) {
            println!("Cache:       {c}MB");
        }
    } else if let Some(error) = resp.get("error") {
        let code = error
            .get("code")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let msg = error
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error");
        bail!("status error [{code}]: {msg}");
    }

    Ok(())
}
