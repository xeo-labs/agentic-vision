//! Cross-site queries — merge results from multiple SiteMaps.

use crate::map::types::{NodeMatch, NodeQuery, SiteMap};

/// A match from a cross-site query, including domain attribution.
#[derive(Debug, Clone)]
pub struct CrossSiteMatch {
    /// The domain this match came from.
    pub domain: String,
    /// The underlying node match.
    pub node_match: NodeMatch,
}

/// Query across multiple maps and return unified results.
pub fn merge_results(
    maps: &[(&str, &SiteMap)],
    query: &NodeQuery,
    limit: usize,
) -> Vec<CrossSiteMatch> {
    let mut all_matches: Vec<CrossSiteMatch> = Vec::new();

    for &(domain, map) in maps {
        let matches = map.filter(query);
        for m in matches {
            all_matches.push(CrossSiteMatch {
                domain: domain.to_string(),
                node_match: m,
            });
        }
    }

    // Sort by confidence descending
    all_matches.sort_by(|a, b| {
        b.node_match
            .confidence
            .partial_cmp(&a.node_match.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    all_matches.truncate(limit);
    all_matches
}

/// Find similar pages across multiple sites.
pub fn cross_site_nearest(
    maps: &[(&str, &SiteMap)],
    target: &[f32; 128],
    k: usize,
) -> Vec<CrossSiteMatch> {
    let mut all_matches: Vec<CrossSiteMatch> = Vec::new();

    for &(domain, map) in maps {
        let matches = map.nearest(target, k);
        for m in matches {
            all_matches.push(CrossSiteMatch {
                domain: domain.to_string(),
                node_match: m,
            });
        }
    }

    // Sort by similarity descending
    all_matches.sort_by(|a, b| {
        let sim_a = a.node_match.similarity.unwrap_or(0.0);
        let sim_b = b.node_match.similarity.unwrap_or(0.0);
        sim_b
            .partial_cmp(&sim_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    all_matches.truncate(k);
    all_matches
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::builder::SiteMapBuilder;
    use crate::map::types::PageType;

    fn make_test_map(domain: &str, page_type: PageType, feature_val: f32) -> SiteMap {
        let mut builder = SiteMapBuilder::new(domain);
        let mut features = [0.0f32; 128];
        features[0] = feature_val;
        builder.add_node(&format!("https://{}/", domain), page_type, features, 230);
        builder.build()
    }

    #[test]
    fn test_cross_site_query() {
        let map_a = make_test_map("a.com", PageType::ProductDetail, 1.0);
        let map_b = make_test_map("b.com", PageType::ProductDetail, 0.5);

        let query = NodeQuery {
            page_types: Some(vec![PageType::ProductDetail]),
            ..Default::default()
        };

        let results = merge_results(&[("a.com", &map_a), ("b.com", &map_b)], &query, 10);
        assert_eq!(results.len(), 2);

        // Both domains represented
        let domains: Vec<&str> = results.iter().map(|r| r.domain.as_str()).collect();
        assert!(domains.contains(&"a.com"));
        assert!(domains.contains(&"b.com"));
    }
}
