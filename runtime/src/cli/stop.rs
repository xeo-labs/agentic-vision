//! Stop the running Cortex daemon.

use crate::cli::start::pid_file_path;
use anyhow::{bail, Context, Result};
use std::time::Duration;

/// Stop the Cortex daemon by reading PID file and sending SIGTERM.
pub async fn run() -> Result<()> {
    let pid_path = pid_file_path();

    if !pid_path.exists() {
        bail!(
            "Cortex is not running (no PID file at {})",
            pid_path.display()
        );
    }

    let pid_str =
        std::fs::read_to_string(&pid_path).context("failed to read PID file")?;
    let pid: i32 = pid_str.trim().parse().context("invalid PID in PID file")?;

    println!("Stopping Cortex (PID {pid})...");

    // Send SIGTERM
    #[cfg(unix)]
    {
        use std::process::Command;
        let output = Command::new("kill")
            .arg(pid.to_string())
            .output()
            .context("failed to send SIGTERM")?;
        if !output.status.success() {
            let _ = std::fs::remove_file(&pid_path);
            bail!("failed to send SIGTERM to PID {pid} (process may have already exited)");
        }
    }

    // Wait up to 5 seconds for the process to exit
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        // Check if process still exists
        #[cfg(unix)]
        {
            use std::process::Command;
            let output = Command::new("kill")
                .args(["-0", &pid.to_string()])
                .output();
            match output {
                Ok(o) if !o.status.success() => {
                    println!("Cortex stopped.");
                    let _ = std::fs::remove_file(&pid_path);
                    let _ = std::fs::remove_file("/tmp/cortex.sock");
                    return Ok(());
                }
                _ => {}
            }
        }
    }

    // Clean up PID file anyway
    let _ = std::fs::remove_file(&pid_path);
    println!("Warning: Cortex may still be running. PID file removed.");
    Ok(())
}
