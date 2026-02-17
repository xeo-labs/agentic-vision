//! Map caching — store and retrieve serialized SiteMaps.

use crate::map::types::SiteMap;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Cache entry with metadata.
struct CacheEntry {
    /// Path to the serialized map file.
    path: PathBuf,
    /// When the entry was cached.
    cached_at: SystemTime,
    /// Cache TTL.
    ttl: Duration,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        SystemTime::now()
            .duration_since(self.cached_at)
            .map(|elapsed| elapsed > self.ttl)
            .unwrap_or(true)
    }
}

/// Map cache backed by the filesystem.
pub struct MapCache {
    /// Base directory for cached maps.
    cache_dir: PathBuf,
    /// In-memory index of cached maps.
    index: HashMap<String, CacheEntry>,
    /// Default TTL for cached maps.
    default_ttl: Duration,
}

impl MapCache {
    /// Create a new map cache in the given directory.
    pub fn new(cache_dir: PathBuf, default_ttl: Duration) -> Result<Self> {
        fs::create_dir_all(&cache_dir)
            .with_context(|| format!("failed to create cache dir: {}", cache_dir.display()))?;

        Ok(Self {
            cache_dir,
            index: HashMap::new(),
            default_ttl,
        })
    }

    /// Create a cache with default settings (~/.cortex/maps/, 1 hour TTL).
    pub fn default_cache() -> Result<Self> {
        let cache_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(".cortex")
            .join("maps");
        Self::new(cache_dir, Duration::from_secs(3600))
    }

    /// Get a cached map for the domain if it exists and is fresh.
    pub fn get(&self, domain: &str) -> Option<&Path> {
        let entry = self.index.get(domain)?;
        if entry.is_expired() {
            return None;
        }
        Some(&entry.path)
    }

    /// Cache a serialized map for the domain.
    pub fn put(&mut self, domain: &str, data: &[u8]) -> Result<PathBuf> {
        let filename = format!("{}.ctx", domain.replace(':', "_"));
        let path = self.cache_dir.join(&filename);

        fs::write(&path, data)
            .with_context(|| format!("failed to write cache file: {}", path.display()))?;

        self.index.insert(
            domain.to_string(),
            CacheEntry {
                path: path.clone(),
                cached_at: SystemTime::now(),
                ttl: self.default_ttl,
            },
        );

        Ok(path)
    }

    /// Cache a SiteMap by serializing it.
    pub fn cache_map(&mut self, domain: &str, map: &SiteMap) -> Result<PathBuf> {
        let data = map.serialize();
        self.put(domain, &data)
    }

    /// Load a cached SiteMap for the domain.
    pub fn load_map(&self, domain: &str) -> Result<Option<SiteMap>> {
        let path = match self.get(domain) {
            Some(p) => p,
            None => return Ok(None),
        };

        let data = fs::read(path)
            .with_context(|| format!("failed to read cached map: {}", path.display()))?;

        let map = SiteMap::deserialize(&data)
            .with_context(|| format!("failed to deserialize cached map for {}", domain))?;

        Ok(Some(map))
    }

    /// Invalidate (remove) a cached map.
    pub fn invalidate(&mut self, domain: &str) {
        if let Some(entry) = self.index.remove(domain) {
            let _ = fs::remove_file(&entry.path);
        }
    }

    /// Number of cached maps (including expired).
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Remove all expired entries.
    pub fn cleanup_expired(&mut self) {
        let expired: Vec<String> = self
            .index
            .iter()
            .filter(|(_, entry)| entry.is_expired())
            .map(|(domain, _)| domain.clone())
            .collect();

        for domain in expired {
            self.invalidate(&domain);
        }
    }

    /// Cache directory path.
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::builder::SiteMapBuilder;
    use crate::map::types::PageType;

    #[test]
    fn test_cache_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut cache = MapCache::new(dir.path().to_path_buf(), Duration::from_secs(3600)).unwrap();

        // Build a small test map
        let mut builder = SiteMapBuilder::new("test.com");
        let features = [0.0f32; 128];
        builder.add_node("https://test.com/", PageType::Home, features, 230);
        let map = builder.build();

        // Cache it
        let path = cache.cache_map("test.com", &map).unwrap();
        assert!(path.exists());

        // Load it back
        let loaded = cache.load_map("test.com").unwrap().unwrap();
        assert_eq!(loaded.nodes.len(), map.nodes.len());
    }

    #[test]
    fn test_cache_invalidation() {
        let dir = tempfile::tempdir().unwrap();
        let mut cache = MapCache::new(dir.path().to_path_buf(), Duration::from_secs(3600)).unwrap();

        cache.put("test.com", b"test data").unwrap();
        assert!(cache.get("test.com").is_some());

        cache.invalidate("test.com");
        assert!(cache.get("test.com").is_none());
    }

    #[test]
    fn test_cache_expiry() {
        let dir = tempfile::tempdir().unwrap();
        let mut cache = MapCache::new(dir.path().to_path_buf(), Duration::from_secs(0)).unwrap();

        cache.put("test.com", b"test data").unwrap();
        // With 0-second TTL, entry is immediately expired
        assert!(cache.get("test.com").is_none());
    }
}
