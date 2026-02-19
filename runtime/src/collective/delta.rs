//! Delta format for incremental map updates.
//!
//! Computes compact diffs between SiteMap versions, enabling efficient sync
//! without retransmitting full maps.

use crate::compiler::models::ModelField;
use crate::map::types::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A delta between two versions of a SiteMap.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapDelta {
    /// Domain this delta applies to.
    pub domain: String,
    /// Hash of the base map this delta applies to.
    pub base_hash: [u8; 32],
    /// When this delta was computed.
    pub timestamp: DateTime<Utc>,
    /// Which Cortex instance produced this delta.
    pub cortex_instance_id: String,
    /// New nodes added.
    pub nodes_added: Vec<CompactNode>,
    /// Node indices removed.
    pub nodes_removed: Vec<u32>,
    /// Nodes with changed features.
    pub nodes_modified: Vec<(u32, FeatureDelta)>,
    /// New edges added (source, target).
    pub edges_added: Vec<(u32, u32)>,
    /// Edges removed (source, target).
    pub edges_removed: Vec<(u32, u32)>,
    /// Schema changes, if any.
    pub schema_delta: Option<SchemaDelta>,
}

/// A compact node representation for deltas (sparse features).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactNode {
    /// FNV hash of the URL.
    pub url_hash: u64,
    /// URL string.
    pub url: String,
    /// Page type byte.
    pub page_type: u8,
    /// Only non-zero feature dimensions.
    pub features: Vec<(u8, f32)>,
}

/// Changed feature dimensions for a modified node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureDelta {
    /// (dimension, new_value) pairs.
    pub changed_dims: Vec<(u8, f32)>,
}

/// Schema-level changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaDelta {
    /// New fields discovered: (model_name, field).
    pub new_fields: Vec<(String, ModelField)>,
    /// Fields removed: (model_name, field_name).
    pub removed_fields: Vec<(String, String)>,
}

/// Compute a delta between an old and new version of a SiteMap.
pub fn compute_delta(old_map: &SiteMap, new_map: &SiteMap, instance_id: &str) -> MapDelta {
    let mut nodes_added: Vec<CompactNode> = Vec::new();
    let mut nodes_removed: Vec<u32> = Vec::new();
    let mut nodes_modified: Vec<(u32, FeatureDelta)> = Vec::new();
    let mut edges_added: Vec<(u32, u32)> = Vec::new();
    let mut edges_removed: Vec<(u32, u32)> = Vec::new();

    // Build URL → index maps for both
    let old_url_index: std::collections::HashMap<&str, usize> = old_map
        .urls
        .iter()
        .enumerate()
        .map(|(i, u)| (u.as_str(), i))
        .collect();
    let new_url_index: std::collections::HashMap<&str, usize> = new_map
        .urls
        .iter()
        .enumerate()
        .map(|(i, u)| (u.as_str(), i))
        .collect();

    // Find added and modified nodes
    for (new_idx, url) in new_map.urls.iter().enumerate() {
        if let Some(&old_idx) = old_url_index.get(url.as_str()) {
            // Node exists in both — check for modifications
            if new_idx < new_map.features.len() && old_idx < old_map.features.len() {
                let old_feats = &old_map.features[old_idx];
                let new_feats = &new_map.features[new_idx];

                let mut changed: Vec<(u8, f32)> = Vec::new();
                for dim in 0..FEATURE_DIM {
                    let diff = (new_feats[dim] - old_feats[dim]).abs();
                    if diff > 0.001 {
                        changed.push((dim as u8, new_feats[dim]));
                    }
                }

                if !changed.is_empty() {
                    nodes_modified.push((
                        new_idx as u32,
                        FeatureDelta {
                            changed_dims: changed,
                        },
                    ));
                }
            }
        } else {
            // New node
            let features: Vec<(u8, f32)> = if new_idx < new_map.features.len() {
                new_map.features[new_idx]
                    .iter()
                    .enumerate()
                    .filter(|(_, &v)| v != 0.0)
                    .map(|(i, &v)| (i as u8, v))
                    .collect()
            } else {
                Vec::new()
            };

            let page_type = if new_idx < new_map.nodes.len() {
                new_map.nodes[new_idx].page_type as u8
            } else {
                0
            };

            nodes_added.push(CompactNode {
                url_hash: fnv_hash(url.as_bytes()),
                url: url.clone(),
                page_type,
                features,
            });
        }
    }

    // Find removed nodes
    for (old_idx, url) in old_map.urls.iter().enumerate() {
        if !new_url_index.contains_key(url.as_str()) {
            nodes_removed.push(old_idx as u32);
        }
    }

    // Compare edges (simplified: check by source/target pairs)
    let old_edges: std::collections::HashSet<(u32, u32)> = collect_edge_pairs(old_map);
    let new_edges: std::collections::HashSet<(u32, u32)> = collect_edge_pairs(new_map);

    for &(src, tgt) in &new_edges {
        if !old_edges.contains(&(src, tgt)) {
            edges_added.push((src, tgt));
        }
    }
    for &(src, tgt) in &old_edges {
        if !new_edges.contains(&(src, tgt)) {
            edges_removed.push((src, tgt));
        }
    }

    MapDelta {
        domain: new_map.header.domain.clone(),
        base_hash: hash_map(old_map),
        timestamp: Utc::now(),
        cortex_instance_id: instance_id.to_string(),
        nodes_added,
        nodes_removed,
        nodes_modified,
        edges_added,
        edges_removed,
        schema_delta: None,
    }
}

/// Apply a delta to a SiteMap (mutates in place).
pub fn apply_delta(map: &mut SiteMap, delta: &MapDelta) -> anyhow::Result<()> {
    // Apply modifications
    for (idx, feature_delta) in &delta.nodes_modified {
        let idx = *idx as usize;
        if idx < map.features.len() {
            for &(dim, value) in &feature_delta.changed_dims {
                map.features[idx][dim as usize] = value;
            }
        }
    }

    // Note: Adding and removing nodes requires rebuilding CSR indexes,
    // which is more complex. For now, modifications are the primary use case.
    // Full add/remove support would require a SiteMapBuilder-like approach.

    // Update header timestamp
    map.header.mapped_at = delta.timestamp.timestamp() as u64;

    Ok(())
}

/// Compute a content hash of a SiteMap for delta base verification.
pub fn hash_map(map: &SiteMap) -> [u8; 32] {
    use std::hash::{Hash, Hasher};
    let mut hasher = fnv::FnvHasher::default();

    map.header.domain.hash(&mut hasher);
    map.header.node_count.hash(&mut hasher);
    map.header.edge_count.hash(&mut hasher);

    for url in &map.urls {
        url.hash(&mut hasher);
    }

    for feats in &map.features {
        for &f in feats {
            f.to_bits().hash(&mut hasher);
        }
    }

    let h = hasher.finish();
    let mut result = [0u8; 32];
    result[..8].copy_from_slice(&h.to_le_bytes());
    // Fill rest with secondary hash rotations
    for i in 1..4 {
        let rotated = h.rotate_left(i * 16);
        result[i as usize * 8..(i as usize + 1) * 8].copy_from_slice(&rotated.to_le_bytes());
    }
    result
}

/// Serialize a delta to compact binary.
pub fn serialize_delta(delta: &MapDelta) -> Vec<u8> {
    serde_json::to_vec(delta).unwrap_or_default()
}

/// Deserialize a delta from binary.
pub fn deserialize_delta(bytes: &[u8]) -> anyhow::Result<MapDelta> {
    Ok(serde_json::from_slice(bytes)?)
}

/// Collect all edge pairs from a SiteMap.
fn collect_edge_pairs(map: &SiteMap) -> std::collections::HashSet<(u32, u32)> {
    let mut pairs = std::collections::HashSet::new();
    for (src_idx, _) in map.nodes.iter().enumerate() {
        let edge_start = if src_idx < map.edge_index.len() {
            map.edge_index[src_idx] as usize
        } else {
            continue;
        };
        let edge_end = if src_idx + 1 < map.edge_index.len() {
            map.edge_index[src_idx + 1] as usize
        } else {
            map.edges.len()
        };
        for edge_idx in edge_start..edge_end {
            if edge_idx < map.edges.len() {
                pairs.insert((src_idx as u32, map.edges[edge_idx].target_node));
            }
        }
    }
    pairs
}

/// Strip private/session data before sharing.
pub fn strip_private_data(map: &mut SiteMap) {
    // Remove auth-required nodes
    let auth_indices: Vec<usize> = map
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| n.flags.is_auth_required())
        .map(|(i, _)| i)
        .collect();

    // Clear session-specific features (dims 112-127) and privacy-sensitive dims
    for features in &mut map.features {
        features[88] = 0.0; // cookie_consent_blocking
        features[89] = 0.0; // popup_count
                            // Zero all session features (112-127)
        for f in features.iter_mut().skip(112) {
            *f = 0.0;
        }
    }

    // Clear auth-walled node features (zero them out rather than removing)
    for &idx in &auth_indices {
        if idx < map.features.len() {
            map.features[idx] = [0.0; FEATURE_DIM];
        }
    }
}

/// FNV-1a hash for URL hashing.
fn fnv_hash(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::builder::SiteMapBuilder;

    #[test]
    fn test_compute_delta_no_changes() {
        let mut builder = SiteMapBuilder::new("test.com");
        let feats = [0.0f32; FEATURE_DIM];
        builder.add_node("https://test.com/", PageType::Home, feats, 200);
        let map = builder.build();

        let delta = compute_delta(&map, &map, "instance-1");
        assert!(delta.nodes_added.is_empty());
        assert!(delta.nodes_removed.is_empty());
        assert!(delta.nodes_modified.is_empty());
    }

    #[test]
    fn test_compute_delta_detects_feature_change() {
        let mut builder1 = SiteMapBuilder::new("test.com");
        let mut feats = [0.0f32; FEATURE_DIM];
        feats[FEAT_PRICE] = 100.0;
        builder1.add_node("https://test.com/p1", PageType::ProductDetail, feats, 200);
        let map1 = builder1.build();

        let mut builder2 = SiteMapBuilder::new("test.com");
        let mut feats2 = [0.0f32; FEATURE_DIM];
        feats2[FEAT_PRICE] = 89.99;
        builder2.add_node("https://test.com/p1", PageType::ProductDetail, feats2, 200);
        let map2 = builder2.build();

        let delta = compute_delta(&map1, &map2, "instance-1");
        assert_eq!(delta.nodes_modified.len(), 1);
        assert_eq!(delta.nodes_modified[0].0, 0); // node 0 modified
    }

    #[test]
    fn test_compute_delta_detects_new_node() {
        let mut builder1 = SiteMapBuilder::new("test.com");
        let feats = [0.0f32; FEATURE_DIM];
        builder1.add_node("https://test.com/p1", PageType::ProductDetail, feats, 200);
        let map1 = builder1.build();

        let mut builder2 = SiteMapBuilder::new("test.com");
        builder2.add_node("https://test.com/p1", PageType::ProductDetail, feats, 200);
        builder2.add_node("https://test.com/p2", PageType::ProductDetail, feats, 200);
        let map2 = builder2.build();

        let delta = compute_delta(&map1, &map2, "instance-1");
        assert_eq!(delta.nodes_added.len(), 1);
        assert_eq!(delta.nodes_added[0].url, "https://test.com/p2");
    }

    #[test]
    fn test_serialize_deserialize_delta() {
        let delta = MapDelta {
            domain: "test.com".to_string(),
            base_hash: [0u8; 32],
            timestamp: Utc::now(),
            cortex_instance_id: "test".to_string(),
            nodes_added: vec![],
            nodes_removed: vec![],
            nodes_modified: vec![],
            edges_added: vec![],
            edges_removed: vec![],
            schema_delta: None,
        };

        let bytes = serialize_delta(&delta);
        let back = deserialize_delta(&bytes).unwrap();
        assert_eq!(back.domain, "test.com");
    }

    #[test]
    fn test_hash_map_deterministic() {
        let mut builder = SiteMapBuilder::new("test.com");
        let feats = [0.0f32; FEATURE_DIM];
        builder.add_node("https://test.com/", PageType::Home, feats, 200);
        let map = builder.build();

        let h1 = hash_map(&map);
        let h2 = hash_map(&map);
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_strip_private_data() {
        let mut builder = SiteMapBuilder::new("test.com");
        let mut feats = [0.0f32; FEATURE_DIM];
        feats[112] = 5.0; // session_page_count
        feats[113] = 3.0; // session_action_count
        builder.add_node("https://test.com/", PageType::Home, feats, 200);
        let mut map = builder.build();

        strip_private_data(&mut map);
        assert_eq!(map.features[0][112], 0.0);
        assert_eq!(map.features[0][113], 0.0);
    }

    // ── v4 Test Suite: Phase 2A — Delta Computation ──

    #[test]
    fn test_v4_delta_size_smaller_than_full_map() {
        let mut builder1 = SiteMapBuilder::new("shop.com");
        for i in 0..50 {
            let mut feats = [0.0f32; FEATURE_DIM];
            feats[FEAT_PRICE] = 100.0 + i as f32;
            builder1.add_node(
                &format!("https://shop.com/p/{i}"),
                PageType::ProductDetail,
                feats,
                200,
            );
        }
        let map1 = builder1.build();

        // Change only 3 prices
        let mut builder2 = SiteMapBuilder::new("shop.com");
        for i in 0..50 {
            let mut feats = [0.0f32; FEATURE_DIM];
            feats[FEAT_PRICE] = if i < 3 {
                80.0 + i as f32
            } else {
                100.0 + i as f32
            };
            builder2.add_node(
                &format!("https://shop.com/p/{i}"),
                PageType::ProductDetail,
                feats,
                200,
            );
        }
        let map2 = builder2.build();

        let delta = compute_delta(&map1, &map2, "test-instance");
        assert_eq!(delta.nodes_modified.len(), 3, "only 3 prices changed");
        assert!(delta.nodes_added.is_empty());
        assert!(delta.nodes_removed.is_empty());

        // Delta serialized size should be much smaller than full map
        let delta_bytes = serialize_delta(&delta);
        let map_bytes = map1.serialize();
        assert!(
            delta_bytes.len() < map_bytes.len() / 2,
            "delta ({}) should be much smaller than full map ({})",
            delta_bytes.len(),
            map_bytes.len()
        );
    }

    #[test]
    fn test_v4_delta_metadata() {
        let mut builder1 = SiteMapBuilder::new("test.com");
        let feats = [0.0f32; FEATURE_DIM];
        builder1.add_node("https://test.com/", PageType::Home, feats, 200);
        let map1 = builder1.build();

        let mut builder2 = SiteMapBuilder::new("test.com");
        let mut feats2 = [0.0f32; FEATURE_DIM];
        feats2[FEAT_PRICE] = 50.0;
        builder2.add_node("https://test.com/", PageType::Home, feats2, 200);
        let map2 = builder2.build();

        let delta = compute_delta(&map1, &map2, "instance-42");

        assert_eq!(delta.domain, "test.com");
        assert_eq!(delta.cortex_instance_id, "instance-42");
        assert_ne!(delta.base_hash, [0u8; 32], "base_hash should be set");
        // timestamp should be recent
        let age = Utc::now() - delta.timestamp;
        assert!(age.num_seconds() < 10, "timestamp should be recent");
    }

    #[test]
    fn test_v4_delta_roundtrip() {
        let mut builder1 = SiteMapBuilder::new("test.com");
        let mut feats = [0.0f32; FEATURE_DIM];
        feats[FEAT_PRICE] = 100.0;
        builder1.add_node("https://test.com/p1", PageType::ProductDetail, feats, 200);
        let map1 = builder1.build();

        let mut builder2 = SiteMapBuilder::new("test.com");
        let mut feats2 = [0.0f32; FEATURE_DIM];
        feats2[FEAT_PRICE] = 80.0;
        builder2.add_node("https://test.com/p1", PageType::ProductDetail, feats2, 200);
        builder2.add_node("https://test.com/p2", PageType::ProductDetail, feats, 200);
        let map2 = builder2.build();

        let delta = compute_delta(&map1, &map2, "test");
        let bytes = serialize_delta(&delta);
        let back = deserialize_delta(&bytes).unwrap();

        assert_eq!(back.domain, delta.domain);
        assert_eq!(back.nodes_added.len(), delta.nodes_added.len());
        assert_eq!(back.nodes_modified.len(), delta.nodes_modified.len());
    }

    #[test]
    fn test_v4_privacy_strips_all_session_features() {
        let mut builder = SiteMapBuilder::new("test.com");
        let mut feats = [0.0f32; FEATURE_DIM];
        // Set all session features (dims 112-127)
        for dim in 112..=127 {
            feats[dim] = (dim - 111) as f32;
        }
        // Also set auth area flag
        feats[FEAT_IS_AUTH_AREA] = 1.0;
        builder.add_node("https://test.com/account", PageType::Account, feats, 200);
        let mut map = builder.build();

        strip_private_data(&mut map);

        // All session dims should be zeroed
        for dim in 112..=127 {
            assert_eq!(
                map.features[0][dim], 0.0,
                "session dim {dim} should be cleared"
            );
        }
    }

    #[test]
    fn test_v4_delta_detects_removed_nodes() {
        let mut builder1 = SiteMapBuilder::new("test.com");
        let feats = [0.0f32; FEATURE_DIM];
        builder1.add_node("https://test.com/p1", PageType::ProductDetail, feats, 200);
        builder1.add_node("https://test.com/p2", PageType::ProductDetail, feats, 200);
        builder1.add_node("https://test.com/p3", PageType::ProductDetail, feats, 200);
        let map1 = builder1.build();

        let mut builder2 = SiteMapBuilder::new("test.com");
        builder2.add_node("https://test.com/p1", PageType::ProductDetail, feats, 200);
        // p2 and p3 removed
        let map2 = builder2.build();

        let delta = compute_delta(&map1, &map2, "test");
        assert_eq!(
            delta.nodes_removed.len(),
            2,
            "should detect 2 removed nodes"
        );
    }

    #[test]
    fn test_v4_hash_map_changes_with_content() {
        let mut builder1 = SiteMapBuilder::new("test.com");
        let feats = [0.0f32; FEATURE_DIM];
        builder1.add_node("https://test.com/p1", PageType::ProductDetail, feats, 200);
        let map1 = builder1.build();

        let mut builder2 = SiteMapBuilder::new("test.com");
        let mut feats2 = [0.0f32; FEATURE_DIM];
        feats2[FEAT_PRICE] = 50.0;
        builder2.add_node("https://test.com/p1", PageType::ProductDetail, feats2, 200);
        let map2 = builder2.build();

        let h1 = hash_map(&map1);
        let h2 = hash_map(&map2);
        assert_ne!(h1, h2, "different maps should have different hashes");
    }
}
