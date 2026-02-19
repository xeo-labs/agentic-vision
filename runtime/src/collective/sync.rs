//! Remote registry sync — push/pull deltas to remote Cortex registries.
//!
//! Handles communication with remote registry servers over HTTPS.

use crate::collective::delta::{self, MapDelta};
use crate::map::types::SiteMap;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Remote sync client for pushing/pulling to a remote registry.
pub struct RemoteSync {
    /// Remote registry endpoint URL.
    endpoint: String,
    /// This Cortex instance's unique ID.
    instance_id: String,
    /// Optional API key for authenticated writes.
    api_key: Option<String>,
    /// HTTP client.
    client: reqwest::Client,
}

/// A remote registry entry listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteEntry {
    pub domain: String,
    pub latest_timestamp: DateTime<Utc>,
    pub node_count: usize,
}

impl RemoteSync {
    /// Create a new remote sync client.
    pub fn new(endpoint: &str, instance_id: &str, api_key: Option<String>) -> Self {
        Self {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            instance_id: instance_id.to_string(),
            api_key,
            client: reqwest::Client::new(),
        }
    }

    /// Push a delta to the remote registry.
    pub async fn push_delta(&self, domain: &str, delta_data: &MapDelta) -> Result<()> {
        let url = format!("{}/v1/maps/{}/deltas", self.endpoint, domain);
        let body = delta::serialize_delta(delta_data);

        let hash_hex = hex_encode(&delta_data.base_hash);

        let mut req = self
            .client
            .post(&url)
            .header("Content-Type", "application/octet-stream")
            .header("X-Cortex-Instance", &self.instance_id)
            .header("X-Cortex-Base-Hash", &hash_hex)
            .body(body);

        if let Some(ref key) = self.api_key {
            req = req.header("Authorization", format!("Bearer {key}"));
        }

        let resp = req.send().await.context("pushing delta to remote")?;
        if !resp.status().is_success() {
            anyhow::bail!(
                "remote push failed: {} {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            );
        }

        Ok(())
    }

    /// Pull the latest map from the remote registry.
    pub async fn pull_map(&self, domain: &str) -> Result<Option<SiteMap>> {
        let url = format!("{}/v1/maps/{}", self.endpoint, domain);

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("pulling map from remote")?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !resp.status().is_success() {
            anyhow::bail!("remote pull failed: {}", resp.status());
        }

        let bytes = resp.bytes().await?;
        let map = SiteMap::deserialize(&bytes)?;
        Ok(Some(map))
    }

    /// Pull deltas since a given timestamp.
    pub async fn pull_since(&self, domain: &str, since: DateTime<Utc>) -> Result<Vec<MapDelta>> {
        let url = format!(
            "{}/v1/maps/{}/deltas?since={}",
            self.endpoint,
            domain,
            since.to_rfc3339()
        );

        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Ok(Vec::new());
        }

        let bytes = resp.bytes().await?;
        // Response is a JSON array of deltas
        let deltas: Vec<MapDelta> = serde_json::from_slice(&bytes).unwrap_or_default();
        Ok(deltas)
    }

    /// List all available domains on the remote registry.
    pub async fn list_available(&self) -> Result<Vec<RemoteEntry>> {
        let url = format!("{}/v1/maps", self.endpoint);
        let resp = self.client.get(&url).send().await?;

        if !resp.status().is_success() {
            return Ok(Vec::new());
        }

        let entries: Vec<RemoteEntry> = resp.json().await.unwrap_or_default();
        Ok(entries)
    }
}

/// Merkle node for efficient sync between registries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleNode {
    /// Hash of this node's subtree.
    pub hash: [u8; 32],
    /// Alphabetical domain range this node covers.
    pub domain_range: (String, String),
    /// Children (None for leaf nodes).
    pub children: Option<(Box<MerkleNode>, Box<MerkleNode>)>,
}

impl MerkleNode {
    /// Build a Merkle tree from a sorted list of (domain, hash) pairs.
    pub fn build(entries: &[(String, [u8; 32])]) -> Option<Self> {
        if entries.is_empty() {
            return None;
        }
        if entries.len() == 1 {
            return Some(MerkleNode {
                hash: entries[0].1,
                domain_range: (entries[0].0.clone(), entries[0].0.clone()),
                children: None,
            });
        }

        let mid = entries.len() / 2;
        let left = Self::build(&entries[..mid]);
        let right = Self::build(&entries[mid..]);

        match (left, right) {
            (Some(l), Some(r)) => {
                let combined_hash = combine_hashes(&l.hash, &r.hash);
                Some(MerkleNode {
                    hash: combined_hash,
                    domain_range: (l.domain_range.0.clone(), r.domain_range.1.clone()),
                    children: Some((Box::new(l), Box::new(r))),
                })
            }
            (Some(node), None) | (None, Some(node)) => Some(node),
            (None, None) => None,
        }
    }

    /// Find domains that differ between two Merkle trees.
    pub fn diff(a: &MerkleNode, b: &MerkleNode) -> Vec<String> {
        if a.hash == b.hash {
            return Vec::new();
        }

        match (&a.children, &b.children) {
            (Some((al, ar)), Some((bl, br))) => {
                let mut diffs = Self::diff(al, bl);
                diffs.extend(Self::diff(ar, br));
                diffs
            }
            _ => {
                // Leaf level — return the domain range
                vec![a.domain_range.0.clone()]
            }
        }
    }
}

/// Combine two hashes for Merkle tree internal nodes.
fn combine_hashes(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let mut result = [0u8; 32];
    for i in 0..32 {
        result[i] = a[i] ^ b[i];
    }
    // Mix with rotation for better distribution
    for i in 0..31 {
        result[i] = result[i].wrapping_add(result[i + 1]);
    }
    result
}

/// Encode bytes as hex string.
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merkle_tree_build() {
        let entries = vec![
            ("a.com".to_string(), [1u8; 32]),
            ("b.com".to_string(), [2u8; 32]),
            ("c.com".to_string(), [3u8; 32]),
        ];

        let tree = MerkleNode::build(&entries);
        assert!(tree.is_some());

        let tree = tree.unwrap();
        assert_eq!(tree.domain_range.0, "a.com");
        assert_eq!(tree.domain_range.1, "c.com");
    }

    #[test]
    fn test_merkle_diff_identical() {
        let entries = vec![
            ("a.com".to_string(), [1u8; 32]),
            ("b.com".to_string(), [2u8; 32]),
        ];

        let tree_a = MerkleNode::build(&entries).unwrap();
        let tree_b = MerkleNode::build(&entries).unwrap();

        let diffs = MerkleNode::diff(&tree_a, &tree_b);
        assert!(diffs.is_empty());
    }

    #[test]
    fn test_merkle_diff_detects_change() {
        let entries_a = vec![
            ("a.com".to_string(), [1u8; 32]),
            ("b.com".to_string(), [2u8; 32]),
        ];
        let entries_b = vec![
            ("a.com".to_string(), [1u8; 32]),
            ("b.com".to_string(), [9u8; 32]), // changed
        ];

        let tree_a = MerkleNode::build(&entries_a).unwrap();
        let tree_b = MerkleNode::build(&entries_b).unwrap();

        let diffs = MerkleNode::diff(&tree_a, &tree_b);
        assert!(!diffs.is_empty());
    }

    #[test]
    fn test_hex_encode() {
        assert_eq!(hex_encode(&[0xDE, 0xAD]), "dead");
        assert_eq!(hex_encode(&[0x00, 0xFF]), "00ff");
    }

    #[test]
    fn test_merkle_empty() {
        let tree = MerkleNode::build(&[]);
        assert!(tree.is_none());
    }

    #[test]
    fn test_merkle_single() {
        let entries = vec![("x.com".to_string(), [42u8; 32])];
        let tree = MerkleNode::build(&entries).unwrap();
        assert_eq!(tree.domain_range.0, "x.com");
        assert!(tree.children.is_none());
    }
}
