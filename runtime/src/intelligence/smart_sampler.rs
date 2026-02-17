//! Smart sampler — improved sampling strategy based on structural importance.

use crate::map::types::PageType;
use std::collections::HashMap;

/// A URL with its classification and structural metadata.
#[derive(Debug, Clone)]
pub struct ClassifiedUrl {
    pub url: String,
    pub page_type: PageType,
    pub confidence: f32,
    /// Number of inbound links from other pages.
    pub inbound_count: u32,
    /// Depth from root (number of path segments).
    pub depth: u32,
}

/// Select the best samples using structural importance.
pub fn select_smart_samples(
    urls: &[ClassifiedUrl],
    max_render: usize,
) -> Vec<String> {
    if urls.is_empty() || max_render == 0 {
        return Vec::new();
    }

    let mut selected: Vec<(usize, f32)> = Vec::new(); // (index, score)
    let mut selected_set = std::collections::HashSet::new();

    // Always include root/home
    if let Some(home_idx) = urls.iter().position(|u| {
        u.url.ends_with('/') || u.page_type == PageType::Home
    }) {
        selected.push((home_idx, f32::MAX));
        selected_set.insert(home_idx);
    }

    // Count page types and determine underrepresented ones
    let mut type_counts: HashMap<PageType, usize> = HashMap::new();
    for u in urls {
        *type_counts.entry(u.page_type).or_insert(0) += 1;
    }

    // Score remaining URLs
    let mut scored: Vec<(usize, f32)> = urls
        .iter()
        .enumerate()
        .filter(|(i, _)| !selected_set.contains(i))
        .map(|(i, u)| {
            let score = compute_sample_score(u, &type_counts);
            (i, score)
        })
        .collect();

    // Sort by score descending
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    // Ensure at least 2 per type
    let mut type_selected: HashMap<PageType, usize> = HashMap::new();
    for &(idx, _) in &selected {
        *type_selected.entry(urls[idx].page_type).or_insert(0) += 1;
    }

    // First pass: fill underrepresented types
    for &(idx, score) in &scored {
        if selected.len() >= max_render {
            break;
        }
        let pt = urls[idx].page_type;
        let count = type_selected.get(&pt).copied().unwrap_or(0);
        if count < 2 {
            selected.push((idx, score));
            selected_set.insert(idx);
            *type_selected.entry(pt).or_insert(0) += 1;
        }
    }

    // Second pass: fill remaining by score
    for &(idx, score) in &scored {
        if selected.len() >= max_render {
            break;
        }
        if !selected_set.contains(&idx) {
            selected.push((idx, score));
            selected_set.insert(idx);
        }
    }

    selected.iter().map(|&(idx, _)| urls[idx].url.clone()).collect()
}

/// Compute a sampling score for a URL.
fn compute_sample_score(
    url: &ClassifiedUrl,
    type_counts: &HashMap<PageType, usize>,
) -> f32 {
    let mut score = 0.0f32;

    // Structural importance: inbound links
    score += url.inbound_count as f32 * 3.0;

    // Closeness to root
    score += 10.0 / (1.0 + url.depth as f32);

    // Classification confidence
    score += url.confidence * 2.0;

    // Underrepresented type boost
    let type_count = type_counts.get(&url.page_type).copied().unwrap_or(0);
    if type_count < 5 {
        score += 5.0;
    }

    score
}
