//! JSONL audit logger — append-only log of all operations.
//!
//! Features:
//! - Append-only JSONL format for easy parsing
//! - Automatic log rotation when file exceeds `MAX_LOG_SIZE` (100MB)
//! - Rotated files named `.1`, `.2`, etc. (max 5 rotations)

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

/// Maximum audit log size before rotation (100 MB).
const MAX_LOG_SIZE: u64 = 100 * 1024 * 1024;

/// Maximum number of rotated log files to keep.
const MAX_ROTATIONS: u32 = 5;

/// A single audit event.
#[derive(Debug, Clone, Serialize)]
pub struct AuditEvent {
    pub timestamp: String,
    pub method: String,
    pub domain: Option<String>,
    pub url: Option<String>,
    pub session_id: Option<String>,
    pub duration_ms: u64,
    pub status: String,
}

/// Append-only JSONL audit logger with automatic rotation.
pub struct AuditLogger {
    file: File,
    path: PathBuf,
    /// Approximate current size (may drift slightly; re-checked on rotation).
    current_size: u64,
}

impl AuditLogger {
    /// Open or create the audit log file.
    pub fn open(path: &PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .with_context(|| format!("failed to open audit log: {}", path.display()))?;

        let current_size = file.metadata().map(|m| m.len()).unwrap_or(0);

        Ok(Self {
            file,
            path: path.clone(),
            current_size,
        })
    }

    /// Open the default audit log at ~/.cortex/audit.jsonl.
    pub fn default_logger() -> Result<Self> {
        let path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(".cortex")
            .join("audit.jsonl");
        Self::open(&path)
    }

    /// Log an audit event.
    pub fn log(&mut self, event: &AuditEvent) -> Result<()> {
        // Check if rotation is needed before writing
        if self.current_size >= MAX_LOG_SIZE {
            self.rotate()?;
        }

        let json = serde_json::to_string(event)?;
        let bytes_written = writeln!(self.file, "{json}")
            .map(|()| json.len() as u64 + 1)
            .unwrap_or(0);
        self.current_size += bytes_written;
        Ok(())
    }

    /// Log a method call with timing.
    pub fn log_method(
        &mut self,
        method: &str,
        domain: Option<&str>,
        url: Option<&str>,
        session_id: Option<&str>,
        duration_ms: u64,
        status: &str,
    ) -> Result<()> {
        self.log(&AuditEvent {
            timestamp: Utc::now().to_rfc3339(),
            method: method.to_string(),
            domain: domain.map(String::from),
            url: url.map(String::from),
            session_id: session_id.map(String::from),
            duration_ms,
            status: status.to_string(),
        })
    }

    /// Rotate log files: audit.jsonl → audit.jsonl.1, .1 → .2, etc.
    fn rotate(&mut self) -> Result<()> {
        // Close current file by dropping and reopening later
        self.file.flush()?;

        // Shift existing rotated files
        for i in (1..MAX_ROTATIONS).rev() {
            let from = rotation_path(&self.path, i);
            let to = rotation_path(&self.path, i + 1);
            if from.exists() {
                let _ = std::fs::rename(&from, &to);
            }
        }

        // Rename current → .1
        let first_rotation = rotation_path(&self.path, 1);
        let _ = std::fs::rename(&self.path, &first_rotation);

        // Delete oldest if over limit
        let oldest = rotation_path(&self.path, MAX_ROTATIONS);
        if oldest.exists() {
            let _ = std::fs::remove_file(&oldest);
        }

        // Reopen fresh log
        self.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .with_context(|| "failed to reopen audit log after rotation")?;
        self.current_size = 0;

        Ok(())
    }
}

/// Build path for a rotated log file: `audit.jsonl.1`, `audit.jsonl.2`, etc.
fn rotation_path(base: &std::path::Path, index: u32) -> PathBuf {
    let name = format!(
        "{}.{index}",
        base.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("audit.jsonl")
    );
    base.with_file_name(name)
}
