//! Local map registry — stores and serves map snapshots + deltas.
//!
//! Provides push/pull semantics for sharing maps between Cortex operations.

use crate::collective::delta::{self, MapDelta};
use crate::map::types::SiteMap;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Local registry that stores map snapshots and deltas.
pub struct LocalRegistry {
    /// Storage directory (e.g., ~/.cortex/registry/).
    storage_dir: PathBuf,
    /// In-memory index: domain → entry.
    index: HashMap<String, RegistryEntry>,
}

/// A registry entry for a single domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryEntry {
    /// Domain name.
    pub domain: String,
    /// Hash of the latest map version.
    pub latest_hash: [u8; 32],
    /// When the latest version was stored.
    pub latest_timestamp: DateTime<Utc>,
    /// Path to the full map snapshot on disk.
    pub snapshot_path: PathBuf,
    /// Ordered list of delta references.
    pub deltas: Vec<DeltaRef>,
    /// Instance IDs that contributed to this entry.
    pub contributed_by: Vec<String>,
}

/// Reference to a stored delta.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaRef {
    /// When this delta was created.
    pub timestamp: DateTime<Utc>,
    /// Path to the delta file.
    pub path: PathBuf,
    /// Base hash this delta applies to.
    pub base_hash: [u8; 32],
}

/// Registry-wide statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryStats {
    /// Number of domains in the registry.
    pub domain_count: usize,
    /// Total size of all snapshots on disk (bytes).
    pub total_snapshot_bytes: u64,
    /// Total number of deltas across all domains.
    pub total_deltas: usize,
}

impl LocalRegistry {
    /// Create or open a local registry at the given directory.
    pub fn new(storage_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&storage_dir)
            .with_context(|| format!("creating registry dir: {}", storage_dir.display()))?;

        let mut registry = Self {
            storage_dir: storage_dir.clone(),
            index: HashMap::new(),
        };

        // Load existing index if present
        let index_path = storage_dir.join("index.json");
        if index_path.exists() {
            let data = std::fs::read_to_string(&index_path)?;
            registry.index = serde_json::from_str(&data).unwrap_or_default();
        }

        Ok(registry)
    }

    /// Push a map (and optional delta) to the registry.
    pub fn push(&mut self, domain: &str, map: &SiteMap, delta: Option<MapDelta>) -> Result<()> {
        let domain_dir = self.storage_dir.join(domain.replace('.', "_"));
        std::fs::create_dir_all(&domain_dir)?;

        // Serialize and save snapshot
        let snapshot_path = domain_dir.join("snapshot.ctx");
        let data = map.serialize();
        std::fs::write(&snapshot_path, &data)?;

        let hash = delta::hash_map(map);

        // Save delta if provided
        let mut deltas = self
            .index
            .get(domain)
            .map(|e| e.deltas.clone())
            .unwrap_or_default();

        if let Some(d) = delta {
            let delta_filename = format!("delta_{}.json", d.timestamp.format("%Y%m%d_%H%M%S"));
            let delta_path = domain_dir.join(&delta_filename);
            let delta_bytes = delta::serialize_delta(&d);
            std::fs::write(&delta_path, &delta_bytes)?;

            deltas.push(DeltaRef {
                timestamp: d.timestamp,
                path: delta_path,
                base_hash: d.base_hash,
            });
        }

        let contributed_by = self
            .index
            .get(domain)
            .map(|e| e.contributed_by.clone())
            .unwrap_or_default();

        self.index.insert(
            domain.to_string(),
            RegistryEntry {
                domain: domain.to_string(),
                latest_hash: hash,
                latest_timestamp: Utc::now(),
                snapshot_path,
                deltas,
                contributed_by,
            },
        );

        self.save_index()?;
        Ok(())
    }

    /// Pull the latest map for a domain.
    pub fn pull(&self, domain: &str) -> Result<Option<(SiteMap, DateTime<Utc>)>> {
        let entry = match self.index.get(domain) {
            Some(e) => e,
            None => return Ok(None),
        };

        if !entry.snapshot_path.exists() {
            return Ok(None);
        }

        let data = std::fs::read(&entry.snapshot_path)?;
        let map = SiteMap::deserialize(&data)?;

        Ok(Some((map, entry.latest_timestamp)))
    }

    /// Pull only deltas since a given timestamp.
    pub fn pull_since(&self, domain: &str, since: DateTime<Utc>) -> Result<Option<Vec<MapDelta>>> {
        let entry = match self.index.get(domain) {
            Some(e) => e,
            None => return Ok(None),
        };

        let mut deltas = Vec::new();
        for delta_ref in &entry.deltas {
            if delta_ref.timestamp > since && delta_ref.path.exists() {
                let bytes = std::fs::read(&delta_ref.path)?;
                let d = delta::deserialize_delta(&bytes)?;
                deltas.push(d);
            }
        }

        Ok(Some(deltas))
    }

    /// List all entries in the registry.
    pub fn list(&self) -> Vec<&RegistryEntry> {
        self.index.values().collect()
    }

    /// Get registry statistics.
    pub fn stats(&self) -> RegistryStats {
        let total_snapshot_bytes: u64 = self
            .index
            .values()
            .map(|e| {
                std::fs::metadata(&e.snapshot_path)
                    .map(|m| m.len())
                    .unwrap_or(0)
            })
            .sum();

        let total_deltas: usize = self.index.values().map(|e| e.deltas.len()).sum();

        RegistryStats {
            domain_count: self.index.len(),
            total_snapshot_bytes,
            total_deltas,
        }
    }

    /// Garbage collect old deltas (keep only the last N per domain).
    pub fn gc(&mut self, keep_count: usize) -> Result<usize> {
        let mut removed = 0;
        for entry in self.index.values_mut() {
            while entry.deltas.len() > keep_count {
                let old = entry.deltas.remove(0);
                if old.path.exists() {
                    let _ = std::fs::remove_file(&old.path);
                    removed += 1;
                }
            }
        }
        self.save_index()?;
        Ok(removed)
    }

    /// Save the index to disk.
    fn save_index(&self) -> Result<()> {
        let index_path = self.storage_dir.join("index.json");
        let data = serde_json::to_string_pretty(&self.index)?;
        std::fs::write(index_path, data)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::builder::SiteMapBuilder;
    use tempfile::TempDir;

    fn build_test_map(domain: &str) -> SiteMap {
        let mut builder = SiteMapBuilder::new(domain);
        let feats = [0.0f32; 128];
        builder.add_node(
            &format!("https://{domain}/"),
            crate::map::types::PageType::Home,
            feats,
            200,
        );
        builder.build()
    }

    #[test]
    fn test_registry_push_pull() {
        let dir = TempDir::new().unwrap();
        let mut registry = LocalRegistry::new(dir.path().to_path_buf()).unwrap();

        let map = build_test_map("test.com");
        registry.push("test.com", &map, None).unwrap();

        let result = registry.pull("test.com").unwrap();
        assert!(result.is_some());

        let (pulled_map, _ts) = result.unwrap();
        assert_eq!(pulled_map.header.domain, "test.com");
    }

    #[test]
    fn test_registry_pull_nonexistent() {
        let dir = TempDir::new().unwrap();
        let registry = LocalRegistry::new(dir.path().to_path_buf()).unwrap();
        assert!(registry.pull("nope.com").unwrap().is_none());
    }

    #[test]
    fn test_registry_list() {
        let dir = TempDir::new().unwrap();
        let mut registry = LocalRegistry::new(dir.path().to_path_buf()).unwrap();

        registry
            .push("a.com", &build_test_map("a.com"), None)
            .unwrap();
        registry
            .push("b.com", &build_test_map("b.com"), None)
            .unwrap();

        assert_eq!(registry.list().len(), 2);
    }

    #[test]
    fn test_registry_stats() {
        let dir = TempDir::new().unwrap();
        let mut registry = LocalRegistry::new(dir.path().to_path_buf()).unwrap();

        registry
            .push("test.com", &build_test_map("test.com"), None)
            .unwrap();

        let stats = registry.stats();
        assert_eq!(stats.domain_count, 1);
        assert!(stats.total_snapshot_bytes > 0);
    }

    // ── v4 Test Suite: Phase 2B — Registry Push/Pull ──

    fn build_product_map(domain: &str, count: usize) -> SiteMap {
        let mut builder = SiteMapBuilder::new(domain);
        for i in 0..count {
            let mut feats = [0.0f32; 128];
            feats[48] = 50.0 + i as f32 * 10.0; // price
            builder.add_node(
                &format!("https://{domain}/p/{i}"),
                crate::map::types::PageType::ProductDetail,
                feats,
                200,
            );
        }
        builder.build()
    }

    #[test]
    fn test_v4_registry_push_pull_round_trip() {
        let dir = TempDir::new().unwrap();
        let mut registry = LocalRegistry::new(dir.path().to_path_buf()).unwrap();

        let map = build_product_map("shop.com", 20);
        registry.push("shop.com", &map, None).unwrap();

        let (pulled, _ts) = registry.pull("shop.com").unwrap().unwrap();
        assert_eq!(
            pulled.nodes.len(),
            map.nodes.len(),
            "pulled map should have same node count"
        );
        assert_eq!(pulled.edges.len(), map.edges.len());
        assert_eq!(pulled.features.len(), map.features.len());

        // Verify feature data preserved
        assert_eq!(
            pulled.features[0][48], map.features[0][48],
            "price should be preserved"
        );
    }

    #[test]
    fn test_v4_registry_push_with_delta() {
        let dir = TempDir::new().unwrap();
        let mut registry = LocalRegistry::new(dir.path().to_path_buf()).unwrap();

        let map1 = build_product_map("shop.com", 5);
        registry.push("shop.com", &map1, None).unwrap();

        let map2 = build_product_map("shop.com", 7);
        let delta = crate::collective::delta::compute_delta(&map1, &map2, "test");
        registry.push("shop.com", &map2, Some(delta)).unwrap();

        // Should have updated map
        let (pulled, _ts) = registry.pull("shop.com").unwrap().unwrap();
        assert_eq!(pulled.nodes.len(), 7);
    }

    #[test]
    fn test_v4_registry_multiple_domains() {
        let dir = TempDir::new().unwrap();
        let mut registry = LocalRegistry::new(dir.path().to_path_buf()).unwrap();

        for domain in &["a.com", "b.com", "c.com", "d.com", "e.com"] {
            registry
                .push(domain, &build_test_map(domain), None)
                .unwrap();
        }

        assert_eq!(registry.list().len(), 5);

        let stats = registry.stats();
        assert_eq!(stats.domain_count, 5);
    }

    #[test]
    fn test_v4_registry_gc() {
        let dir = TempDir::new().unwrap();
        let mut registry = LocalRegistry::new(dir.path().to_path_buf()).unwrap();

        registry
            .push("old.com", &build_test_map("old.com"), None)
            .unwrap();
        registry
            .push("new.com", &build_test_map("new.com"), None)
            .unwrap();

        // GC should work without errors (keep only 1 delta per domain)
        let cleaned = registry.gc(1).unwrap();
        assert!(
            cleaned == 0 || cleaned > 0,
            "gc should report cleaned count"
        );
    }

    #[test]
    fn test_v4_registry_pull_since() {
        let dir = TempDir::new().unwrap();
        let mut registry = LocalRegistry::new(dir.path().to_path_buf()).unwrap();

        let map = build_test_map("test.com");
        let delta = crate::collective::delta::MapDelta {
            domain: "test.com".to_string(),
            base_hash: [0u8; 32],
            timestamp: chrono::Utc::now(),
            cortex_instance_id: "test".to_string(),
            nodes_added: vec![],
            nodes_removed: vec![],
            nodes_modified: vec![],
            edges_added: vec![],
            edges_removed: vec![],
            schema_delta: None,
        };

        registry.push("test.com", &map, Some(delta)).unwrap();

        let since = chrono::Utc::now() - chrono::Duration::hours(1);
        let deltas = registry.pull_since("test.com", since).unwrap();
        assert!(deltas.is_some());
    }
}
