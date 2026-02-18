//! SiteMapBuilder for incrementally constructing a SiteMap.

use crate::map::types::*;
use std::time::{SystemTime, UNIX_EPOCH};

/// Intermediate edge data during building.
struct EdgeData {
    from: u32,
    edge: EdgeRecord,
}

/// Intermediate action data during building.
struct ActionData {
    node: u32,
    action: ActionRecord,
}

/// Builder for constructing a SiteMap incrementally.
pub struct SiteMapBuilder {
    domain: String,
    urls: Vec<String>,
    nodes: Vec<NodeRecord>,
    features: Vec<[f32; FEATURE_DIM]>,
    edges: Vec<EdgeData>,
    actions: Vec<ActionData>,
    has_sitemap: bool,
}

impl SiteMapBuilder {
    /// Create a new builder for the given domain.
    pub fn new(domain: &str) -> Self {
        Self {
            domain: domain.to_string(),
            urls: Vec::new(),
            nodes: Vec::new(),
            features: Vec::new(),
            edges: Vec::new(),
            actions: Vec::new(),
            has_sitemap: false,
        }
    }

    /// Set whether the site had a sitemap.xml.
    pub fn set_has_sitemap(&mut self, has: bool) {
        self.has_sitemap = has;
    }

    /// Add a node and return its index.
    pub fn add_node(
        &mut self,
        url: &str,
        page_type: PageType,
        features: [f32; FEATURE_DIM],
        confidence: u8,
    ) -> u32 {
        let index = self.nodes.len() as u32;

        // Compute feature norm (L2)
        let norm: f32 = features.iter().map(|f| f * f).sum::<f32>().sqrt();

        let record = NodeRecord {
            page_type,
            confidence,
            freshness: 0,
            flags: NodeFlags::default(),
            content_hash: 0,
            rendered_at: 0,
            http_status: 0,
            depth: 0,
            inbound_count: 0,
            outbound_count: 0,
            feature_norm: norm,
            reserved: 0,
        };

        self.urls.push(url.to_string());
        self.nodes.push(record);
        self.features.push(features);

        index
    }

    /// Add an edge between two nodes.
    pub fn add_edge(
        &mut self,
        from: u32,
        to: u32,
        edge_type: EdgeType,
        weight: u8,
        flags: EdgeFlags,
    ) {
        self.edges.push(EdgeData {
            from,
            edge: EdgeRecord {
                target_node: to,
                edge_type,
                weight,
                flags,
                reserved: 0,
            },
        });
    }

    /// Add an action available on a node.
    pub fn add_action(
        &mut self,
        node: u32,
        opcode: OpCode,
        target_node: i32,
        cost_hint: u8,
        risk: u8,
    ) {
        self.actions.push(ActionData {
            node,
            action: ActionRecord {
                opcode,
                target_node,
                cost_hint,
                risk,
                http_executable: false,
            },
        });
    }

    /// Add an HTTP-executable action available on a node.
    ///
    /// HTTP-executable actions can be executed via HTTP POST/GET without a browser.
    pub fn add_action_http(
        &mut self,
        node: u32,
        opcode: OpCode,
        target_node: i32,
        cost_hint: u8,
        risk: u8,
    ) {
        self.actions.push(ActionData {
            node,
            action: ActionRecord {
                opcode,
                target_node,
                cost_hint,
                risk,
                http_executable: true,
            },
        });
    }

    /// Mark a node as rendered with updated features and flags.
    pub fn set_rendered(&mut self, node: u32, features: [f32; FEATURE_DIM]) {
        let idx = node as usize;
        if idx < self.nodes.len() {
            self.nodes[idx].flags.0 |= NodeFlags::RENDERED;
            self.nodes[idx].freshness = 255;
            self.features[idx] = features;
            let norm: f32 = features.iter().map(|f| f * f).sum::<f32>().sqrt();
            self.nodes[idx].feature_norm = norm;
        }
    }

    /// Read a single feature dimension for an existing node.
    pub fn get_feature(&self, node: u32, dimension: usize) -> f32 {
        let idx = node as usize;
        if idx < self.features.len() && dimension < FEATURE_DIM {
            self.features[idx][dimension]
        } else {
            0.0
        }
    }

    /// Update a single feature dimension for an existing node.
    ///
    /// Used to patch feature vectors after initial encoding — for example,
    /// to inject HTTP action counts discovered in Layer 2.5.
    pub fn update_feature(&mut self, node: u32, dimension: usize, value: f32) {
        let idx = node as usize;
        if idx < self.features.len() && dimension < FEATURE_DIM {
            self.features[idx][dimension] = value;
        }
    }

    /// Merge additional flag bits into a node's flags.
    ///
    /// Used to set computed flags like `HAS_PRICE`, `HAS_MEDIA`, `HAS_FORM`
    /// after feature encoding.
    pub fn merge_flags(&mut self, node: u32, flags: NodeFlags) {
        let idx = node as usize;
        if idx < self.nodes.len() {
            self.nodes[idx].flags.0 |= flags.0;
        }
    }

    /// Build the final SiteMap.
    pub fn build(mut self) -> SiteMap {
        let node_count = self.nodes.len();

        // Sort edges by source node for CSR format
        self.edges.sort_by_key(|e| e.from);

        // Build edge CSR index
        let mut edge_index = vec![0u32; node_count + 1];
        let mut edges = Vec::with_capacity(self.edges.len());
        for ed in &self.edges {
            edges.push(ed.edge.clone());
        }

        // Count edges per node
        let mut counts = vec![0u32; node_count];
        for ed in &self.edges {
            if (ed.from as usize) < node_count {
                counts[ed.from as usize] += 1;
            }
        }
        // Build prefix sum
        for i in 0..node_count {
            edge_index[i + 1] = edge_index[i] + counts[i];
        }

        // Update inbound/outbound counts
        for ed in &self.edges {
            let from = ed.from as usize;
            let to = ed.edge.target_node as usize;
            if from < node_count {
                self.nodes[from].outbound_count = self.nodes[from].outbound_count.saturating_add(1);
            }
            if to < node_count {
                self.nodes[to].inbound_count = self.nodes[to].inbound_count.saturating_add(1);
            }
        }

        // Sort actions by node for CSR format
        self.actions.sort_by_key(|a| a.node);

        // Build action CSR index
        let mut action_index = vec![0u32; node_count + 1];
        let mut actions = Vec::with_capacity(self.actions.len());
        for ad in &self.actions {
            actions.push(ad.action.clone());
        }
        let mut act_counts = vec![0u32; node_count];
        for ad in &self.actions {
            if (ad.node as usize) < node_count {
                act_counts[ad.node as usize] += 1;
            }
        }
        for i in 0..node_count {
            action_index[i + 1] = action_index[i] + act_counts[i];
        }

        // Compute clusters using basic k-means
        let k = if node_count < 30 {
            1.max(node_count / 3)
        } else {
            3.max(((node_count as f64 / 10.0).sqrt()) as usize)
        };
        let (cluster_assignments, cluster_centroids) = compute_clusters(&self.features, k);

        let mapped_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut flags: u16 = 0;
        if self.has_sitemap {
            flags |= 1;
        }

        let header = MapHeader {
            magic: SITEMAP_MAGIC,
            format_version: FORMAT_VERSION,
            domain: self.domain,
            mapped_at,
            node_count: node_count as u32,
            edge_count: edges.len() as u32,
            cluster_count: cluster_centroids.len() as u16,
            flags,
        };

        SiteMap {
            header,
            nodes: self.nodes,
            edges,
            edge_index,
            features: self.features,
            actions,
            action_index,
            cluster_assignments,
            cluster_centroids,
            urls: self.urls,
        }
    }
}

/// Simple k-means clustering on feature vectors.
fn compute_clusters(
    features: &[[f32; FEATURE_DIM]],
    k: usize,
) -> (Vec<u16>, Vec<[f32; FEATURE_DIM]>) {
    let n = features.len();
    if n == 0 || k == 0 {
        return (Vec::new(), Vec::new());
    }
    let k = k.min(n);

    // Initialize centroids by evenly spacing through the data
    let mut centroids: Vec<[f32; FEATURE_DIM]> = Vec::with_capacity(k);
    for i in 0..k {
        let idx = i * n / k;
        centroids.push(features[idx]);
    }

    let mut assignments = vec![0u16; n];

    // Run k-means for up to 20 iterations
    for _ in 0..20 {
        let mut changed = false;

        // Assign each point to nearest centroid
        for (i, feat) in features.iter().enumerate() {
            let mut best_cluster = 0u16;
            let mut best_dist = f32::MAX;
            for (c, centroid) in centroids.iter().enumerate() {
                let dist: f32 = feat
                    .iter()
                    .zip(centroid.iter())
                    .map(|(a, b)| (a - b) * (a - b))
                    .sum();
                if dist < best_dist {
                    best_dist = dist;
                    best_cluster = c as u16;
                }
            }
            if assignments[i] != best_cluster {
                assignments[i] = best_cluster;
                changed = true;
            }
        }

        if !changed {
            break;
        }

        // Recompute centroids
        let mut sums = vec![[0.0f32; FEATURE_DIM]; k];
        let mut counts = vec![0u32; k];
        for (i, feat) in features.iter().enumerate() {
            let c = assignments[i] as usize;
            counts[c] += 1;
            for (d, &val) in feat.iter().enumerate() {
                sums[c][d] += val;
            }
        }
        for c in 0..k {
            if counts[c] > 0 {
                for (d, sum_val) in sums[c].iter().enumerate() {
                    centroids[c][d] = sum_val / counts[c] as f32;
                }
            }
        }
    }

    (assignments, centroids)
}
