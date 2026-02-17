//! Progressive refinement — continue rendering unrendered nodes in the background.

use crate::map::types::SiteMap;
use std::collections::VecDeque;

/// Information about a node that needs rendering.
#[derive(Debug, Clone)]
pub struct RenderCandidate {
    /// Node index in the SiteMap.
    pub node_index: u32,
    /// URL to render.
    pub url: String,
    /// Priority score (higher = render first).
    pub priority: f32,
}

/// Select unrendered nodes for progressive rendering, ordered by priority.
pub fn select_unrendered(map: &SiteMap, batch_size: usize) -> Vec<RenderCandidate> {
    let mut candidates: Vec<RenderCandidate> = map
        .nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| !node.flags.is_rendered())
        .map(|(i, _node)| {
            let url = map.urls.get(i).cloned().unwrap_or_default();
            let priority = compute_priority(map, i as u32);
            RenderCandidate {
                node_index: i as u32,
                url,
                priority,
            }
        })
        .collect();

    // Sort by priority descending
    candidates.sort_by(|a, b| b.priority.partial_cmp(&a.priority).unwrap_or(std::cmp::Ordering::Equal));
    candidates.truncate(batch_size);
    candidates
}

/// Compute rendering priority for a node.
fn compute_priority(map: &SiteMap, node_index: u32) -> f32 {
    let idx = node_index as usize;
    let mut score = 0.0f32;

    // Inbound link count (higher = more important)
    let inbound = map
        .edges
        .iter()
        .filter(|e| e.target_node == node_index)
        .count() as f32;
    score += inbound * 2.0;

    // Closeness to root (lower depth = higher priority)
    if let Some(url) = map.urls.get(idx) {
        let depth = url.matches('/').count().saturating_sub(2);
        score += 10.0 / (1.0 + depth as f32);
    }

    // Underrepresented page type gets a boost
    let node_type = map.nodes[idx].page_type;
    let type_count = map
        .nodes
        .iter()
        .filter(|n| n.page_type == node_type && n.flags.is_rendered())
        .count();
    if type_count < 2 {
        score += 5.0;
    }

    score
}

/// Iterator-like queue for progressive rendering work.
pub struct ProgressiveQueue {
    queue: VecDeque<RenderCandidate>,
}

impl ProgressiveQueue {
    /// Create a queue from unrendered nodes.
    pub fn from_map(map: &SiteMap, max_items: usize) -> Self {
        let candidates = select_unrendered(map, max_items);
        Self {
            queue: VecDeque::from(candidates),
        }
    }

    /// Take the next candidate to render.
    pub fn next(&mut self) -> Option<RenderCandidate> {
        self.queue.pop_front()
    }

    /// How many candidates remain.
    pub fn remaining(&self) -> usize {
        self.queue.len()
    }

    /// Whether the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}
