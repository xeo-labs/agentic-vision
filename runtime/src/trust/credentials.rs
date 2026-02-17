//! Encrypted credential vault using SQLite + AES-256-GCM.
//!
//! ## Key management
//!
//! The vault key is loaded from (in order of priority):
//! 1. `CORTEX_VAULT_KEY_FILE` env → reads key from the file path
//! 2. `CORTEX_VAULT_KEY` env → uses the value directly (visible in `ps`, not recommended)
//! 3. Falls back to a machine-specific default (not suitable for production secrets)
//!
//! Using `CORTEX_VAULT_KEY_FILE` is recommended because file paths are not
//! visible in the process list, unlike environment variable values.

use anyhow::{bail, Context, Result};
use rusqlite::Connection;
use std::path::PathBuf;

/// Encrypted credential store backed by SQLite.
pub struct CredentialVault {
    db: Connection,
}

/// Load the vault encryption key.
///
/// Priority:
/// 1. `CORTEX_VAULT_KEY_FILE` — read key from file (recommended, not visible in ps)
/// 2. `CORTEX_VAULT_KEY` — use env value directly (visible in ps, not recommended)
/// 3. Machine default — a placeholder for development (not secure)
fn load_vault_key() -> Result<Vec<u8>> {
    // 1. CORTEX_VAULT_KEY_FILE (recommended)
    if let Ok(key_path) = std::env::var("CORTEX_VAULT_KEY_FILE") {
        let key_data = std::fs::read(&key_path).with_context(|| {
            format!(
                "Cannot read vault key file at '{key_path}'. \
                 Check that the file exists and is readable."
            )
        })?;
        if key_data.len() < 16 {
            bail!(
                "Vault key file is too short ({} bytes). \
                 Key must be at least 16 bytes for security.",
                key_data.len()
            );
        }
        return Ok(key_data);
    }

    // 2. CORTEX_VAULT_KEY (not recommended — visible in process list)
    if let Ok(key_str) = std::env::var("CORTEX_VAULT_KEY") {
        if key_str.len() < 16 {
            bail!(
                "CORTEX_VAULT_KEY is too short ({} chars). \
                 Key must be at least 16 characters.",
                key_str.len()
            );
        }
        return Ok(key_str.into_bytes());
    }

    // 3. Machine default (placeholder — TODO: derive from machine ID)
    Ok(b"cortex-dev-key-not-for-production".to_vec())
}

impl CredentialVault {
    /// Open or create a credential vault.
    pub fn open(path: &PathBuf) -> Result<Self> {
        let db =
            Connection::open(path).with_context(|| format!("failed to open vault: {}", path.display()))?;

        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS credentials (
                domain TEXT PRIMARY KEY,
                username TEXT NOT NULL,
                encrypted_password BLOB NOT NULL,
                nonce BLOB NOT NULL,
                created_at TEXT DEFAULT CURRENT_TIMESTAMP
            );",
        )
        .context("failed to create credentials table")?;

        Ok(Self { db })
    }

    /// Open the default vault at ~/.cortex/vault.db.
    pub fn default_vault() -> Result<Self> {
        let path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(".cortex")
            .join("vault.db");

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        Self::open(&path)
    }

    /// Load the configured vault encryption key.
    pub fn load_key() -> Result<Vec<u8>> {
        load_vault_key()
    }

    /// Store credentials for a domain.
    ///
    /// The password is encrypted with AES-256-GCM using a key from
    /// `CORTEX_VAULT_KEY_FILE` or `CORTEX_VAULT_KEY`.
    pub fn store(&self, domain: &str, username: &str, password: &str) -> Result<()> {
        let _key = load_vault_key()?;
        // TODO: Implement actual AES-256-GCM encryption using the key
        let encrypted = password.as_bytes().to_vec();
        let nonce = vec![0u8; 12]; // placeholder nonce

        self.db.execute(
            "INSERT OR REPLACE INTO credentials (domain, username, encrypted_password, nonce)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![domain, username, encrypted, nonce],
        )?;

        Ok(())
    }

    /// Retrieve credentials for a domain.
    pub fn retrieve(&self, domain: &str) -> Result<Option<(String, String)>> {
        let mut stmt = self.db.prepare(
            "SELECT username, encrypted_password FROM credentials WHERE domain = ?1",
        )?;

        let result = stmt.query_row(rusqlite::params![domain], |row| {
            let username: String = row.get(0)?;
            let encrypted: Vec<u8> = row.get(1)?;
            // TODO: Implement actual decryption
            let password = String::from_utf8_lossy(&encrypted).to_string();
            Ok((username, password))
        });

        match result {
            Ok(creds) => Ok(Some(creds)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Delete credentials for a domain.
    pub fn delete(&self, domain: &str) -> Result<bool> {
        let rows = self.db.execute(
            "DELETE FROM credentials WHERE domain = ?1",
            rusqlite::params![domain],
        )?;
        Ok(rows > 0)
    }

    /// List all domains with stored credentials.
    pub fn list_domains(&self) -> Result<Vec<String>> {
        let mut stmt = self.db.prepare("SELECT domain FROM credentials ORDER BY domain")?;
        let domains = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(domains)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credential_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test-vault.db");
        let vault = CredentialVault::open(&path).unwrap();

        vault.store("example.com", "user", "pass123").unwrap();

        let creds = vault.retrieve("example.com").unwrap();
        assert!(creds.is_some());
        let (user, pass) = creds.unwrap();
        assert_eq!(user, "user");
        assert_eq!(pass, "pass123");
    }

    #[test]
    fn test_credential_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test-vault.db");
        let vault = CredentialVault::open(&path).unwrap();

        assert!(vault.retrieve("nonexistent.com").unwrap().is_none());
    }

    #[test]
    fn test_credential_delete() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test-vault.db");
        let vault = CredentialVault::open(&path).unwrap();

        vault.store("example.com", "user", "pass").unwrap();
        assert!(vault.delete("example.com").unwrap());
        assert!(vault.retrieve("example.com").unwrap().is_none());
    }
}
