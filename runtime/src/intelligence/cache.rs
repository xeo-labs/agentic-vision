//! Map caching — store and retrieve serialized SiteMaps.
//!
//! ## LRU eviction
//!
//! When the cache exceeds `max_entries`, the least-recently-accessed entry
//! is evicted (both from the index and from disk).

use crate::map::types::SiteMap;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// Default maximum number of cached maps before LRU eviction.
const DEFAULT_MAX_ENTRIES: usize = 50;

/// Cache entry with metadata.
struct CacheEntry {
    /// Path to the serialized map file.
    path: PathBuf,
    /// When the entry was cached.
    cached_at: SystemTime,
    /// Cache TTL.
    ttl: Duration,
    /// When the entry was last accessed (for LRU).
    last_accessed: Instant,
}

impl CacheEntry {
    fn is_expired(&self) -> bool {
        SystemTime::now()
            .duration_since(self.cached_at)
            .map(|elapsed| elapsed > self.ttl)
            .unwrap_or(true)
    }

    fn touch(&mut self) {
        self.last_accessed = Instant::now();
    }
}

/// Map cache backed by the filesystem with LRU eviction.
pub struct MapCache {
    /// Base directory for cached maps.
    cache_dir: PathBuf,
    /// In-memory index of cached maps.
    index: HashMap<String, CacheEntry>,
    /// Default TTL for cached maps.
    default_ttl: Duration,
    /// Maximum number of cached maps before LRU eviction.
    max_entries: usize,
}

impl MapCache {
    /// Create a new map cache in the given directory.
    ///
    /// On creation, scans the cache directory for existing `.ctx` files and
    /// rebuilds the in-memory index so that previously cached maps are
    /// immediately available for lookup.
    pub fn new(cache_dir: PathBuf, default_ttl: Duration) -> Result<Self> {
        fs::create_dir_all(&cache_dir)
            .with_context(|| format!("failed to create cache dir: {}", cache_dir.display()))?;

        let mut index = HashMap::new();

        // Scan the cache directory for existing .ctx files and rebuild the index.
        if let Ok(entries) = fs::read_dir(&cache_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("ctx") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        // Reverse the filename encoding: underscores back to colons
                        let domain = stem.replace('_', ":");
                        // Use the file's modification time as cached_at
                        let cached_at = entry
                            .metadata()
                            .and_then(|m| m.modified())
                            .unwrap_or_else(|_| SystemTime::now());
                        index.insert(
                            domain,
                            CacheEntry {
                                path,
                                cached_at,
                                ttl: default_ttl,
                                last_accessed: Instant::now(),
                            },
                        );
                    }
                }
            }
        }

        tracing::debug!(
            "MapCache initialized: {} entries from {}",
            index.len(),
            cache_dir.display()
        );

        Ok(Self {
            cache_dir,
            index,
            default_ttl,
            max_entries: DEFAULT_MAX_ENTRIES,
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
    pub fn get(&mut self, domain: &str) -> Option<&Path> {
        // Touch for LRU tracking
        if let Some(entry) = self.index.get_mut(domain) {
            if entry.is_expired() {
                return None;
            }
            entry.touch();
            Some(&entry.path)
        } else {
            None
        }
    }

    /// Cache a serialized map for the domain.
    ///
    /// If the cache is full, the least-recently-used entry is evicted first.
    pub fn put(&mut self, domain: &str, data: &[u8]) -> Result<PathBuf> {
        // Evict LRU entry if at capacity
        if self.index.len() >= self.max_entries && !self.index.contains_key(domain) {
            self.evict_lru();
        }

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
                last_accessed: Instant::now(),
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
    pub fn load_map(&mut self, domain: &str) -> Result<Option<SiteMap>> {
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

    /// Load all cached (non-expired) SiteMaps, returning a domain → SiteMap map.
    pub fn load_all_maps(&mut self) -> Result<HashMap<String, SiteMap>> {
        let domains: Vec<String> = self
            .index
            .iter()
            .filter(|(_, entry)| !entry.is_expired())
            .map(|(domain, _)| domain.clone())
            .collect();

        let mut maps = HashMap::new();
        for domain in domains {
            if let Some(map) = self.load_map(&domain)? {
                maps.insert(domain, map);
            }
        }
        Ok(maps)
    }

    /// Invalidate (remove) a cached map.
    pub fn invalidate(&mut self, domain: &str) {
        if let Some(entry) = self.index.remove(domain) {
            let _ = fs::remove_file(&entry.path);
        }
    }

    /// Evict the least-recently-used cache entry.
    fn evict_lru(&mut self) {
        // First, try to evict expired entries
        let expired: Vec<String> = self
            .index
            .iter()
            .filter(|(_, entry)| entry.is_expired())
            .map(|(domain, _)| domain.clone())
            .collect();

        if !expired.is_empty() {
            for domain in expired {
                self.invalidate(&domain);
            }
            return;
        }

        // Otherwise, find the least-recently-accessed entry
        if let Some(lru_domain) = self
            .index
            .iter()
            .min_by_key(|(_, entry)| entry.last_accessed)
            .map(|(domain, _)| domain.clone())
        {
            tracing::info!("evicting LRU cache entry: {lru_domain}");
            self.invalidate(&lru_domain);
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

    #[test]
    fn test_lru_eviction() {
        let dir = tempfile::tempdir().unwrap();
        let mut cache = MapCache::new(dir.path().to_path_buf(), Duration::from_secs(3600)).unwrap();
        cache.max_entries = 3;

        // Fill cache to capacity
        cache.put("a.com", b"data-a").unwrap();
        cache.put("b.com", b"data-b").unwrap();
        cache.put("c.com", b"data-c").unwrap();
        assert_eq!(cache.len(), 3);

        // Touch a.com and b.com so c.com is LRU (it was the most recently added
        // but never accessed after insert — but a.com was inserted first so
        // by default it has the earliest last_accessed. Let's touch b and c
        // so a becomes LRU.)
        let _ = cache.get("b.com");
        let _ = cache.get("c.com");

        // Adding d.com should evict a.com (the LRU entry)
        cache.put("d.com", b"data-d").unwrap();
        assert_eq!(cache.len(), 3);
        assert!(cache.get("a.com").is_none()); // evicted
        assert!(cache.get("b.com").is_some());
        assert!(cache.get("c.com").is_some());
        assert!(cache.get("d.com").is_some());
    }

    #[test]
    fn test_lru_evicts_expired_first() {
        let dir = tempfile::tempdir().unwrap();
        let mut cache = MapCache::new(dir.path().to_path_buf(), Duration::from_secs(3600)).unwrap();
        cache.max_entries = 3;

        // Add with zero TTL (immediately expired)
        cache.put("expired.com", b"old").unwrap();
        // Manually set TTL to 0 to force expiry
        if let Some(entry) = cache.index.get_mut("expired.com") {
            entry.ttl = Duration::from_secs(0);
        }

        cache.put("b.com", b"data-b").unwrap();
        cache.put("c.com", b"data-c").unwrap();

        // Adding d.com — expired.com should be evicted first (not b.com)
        cache.put("d.com", b"data-d").unwrap();
        assert_eq!(cache.len(), 3);
        assert!(cache.get("expired.com").is_none()); // evicted (was expired)
        assert!(cache.get("b.com").is_some());
        assert!(cache.get("c.com").is_some());
        assert!(cache.get("d.com").is_some());
    }

    #[test]
    fn test_cleanup_expired() {
        let dir = tempfile::tempdir().unwrap();
        let mut cache = MapCache::new(dir.path().to_path_buf(), Duration::from_secs(3600)).unwrap();

        cache.put("a.com", b"data-a").unwrap();
        cache.put("b.com", b"data-b").unwrap();

        // Make a.com expired
        if let Some(entry) = cache.index.get_mut("a.com") {
            entry.ttl = Duration::from_secs(0);
        }

        cache.cleanup_expired();
        assert_eq!(cache.len(), 1);
        assert!(cache.get("a.com").is_none());
        assert!(cache.get("b.com").is_some());
    }
}
