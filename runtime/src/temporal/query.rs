//! Temporal query engine — structured queries over time-series data.

use crate::temporal::store::TemporalStore;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A temporal query specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalQuery {
    /// Domain to query (None = all domains).
    pub domain: Option<String>,
    /// Model type filter (e.g., "Product", "Article").
    pub model_type: Option<String>,
    /// Feature dimension to track.
    pub feature_dim: u8,
    /// Start of time range.
    pub since: DateTime<Utc>,
    /// End of time range (None = now).
    pub until: Option<DateTime<Utc>>,
    /// Aggregation level.
    pub aggregation: Aggregation,
}

/// Time-series aggregation level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Aggregation {
    /// Raw data points.
    Raw,
    /// Aggregate per day.
    Daily(AggFunc),
    /// Aggregate per week.
    Weekly(AggFunc),
    /// Aggregate per month.
    Monthly(AggFunc),
}

/// Aggregation function.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AggFunc {
    Min,
    Max,
    Avg,
    Last,
    First,
}

/// Result of a temporal query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalResult {
    /// Data points: (timestamp, value).
    pub points: Vec<(DateTime<Utc>, f32)>,
    /// Number of raw points before aggregation.
    pub raw_count: usize,
}

/// Execute a temporal query against the store.
pub fn temporal_query(store: &TemporalStore, query: &TemporalQuery) -> Result<TemporalResult> {
    let domain = query.domain.as_deref().unwrap_or("*");

    // Get raw points
    let raw_points = if domain == "*" {
        // All domains — would need registry listing, return empty for now
        Vec::new()
    } else {
        store.history(domain, "", query.feature_dim, query.since)?
    };

    // Filter by time range
    let filtered: Vec<(DateTime<Utc>, f32)> = raw_points
        .into_iter()
        .filter(|(ts, _)| {
            if let Some(until) = query.until {
                *ts <= until
            } else {
                true
            }
        })
        .collect();

    let raw_count = filtered.len();

    // Apply aggregation
    let points = match &query.aggregation {
        Aggregation::Raw => filtered,
        Aggregation::Daily(func) => aggregate_by_period(&filtered, 86400, *func),
        Aggregation::Weekly(func) => aggregate_by_period(&filtered, 604800, *func),
        Aggregation::Monthly(func) => aggregate_by_period(&filtered, 2592000, *func),
    };

    Ok(TemporalResult { points, raw_count })
}

/// Aggregate time series by fixed period.
fn aggregate_by_period(
    points: &[(DateTime<Utc>, f32)],
    period_secs: i64,
    func: AggFunc,
) -> Vec<(DateTime<Utc>, f32)> {
    if points.is_empty() {
        return Vec::new();
    }

    // Group by period bucket
    let mut buckets: std::collections::BTreeMap<i64, Vec<f32>> = std::collections::BTreeMap::new();

    for (ts, val) in points {
        let bucket = ts.timestamp() / period_secs;
        buckets.entry(bucket).or_default().push(*val);
    }

    buckets
        .into_iter()
        .map(|(bucket, values)| {
            let ts = DateTime::from_timestamp(bucket * period_secs, 0).unwrap_or_else(Utc::now);
            let agg_value = apply_agg_func(&values, func);
            (ts, agg_value)
        })
        .collect()
}

/// Apply an aggregation function to a list of values.
fn apply_agg_func(values: &[f32], func: AggFunc) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    match func {
        AggFunc::Min => values.iter().cloned().fold(f32::INFINITY, f32::min),
        AggFunc::Max => values.iter().cloned().fold(f32::NEG_INFINITY, f32::max),
        AggFunc::Avg => values.iter().sum::<f32>() / values.len() as f32,
        AggFunc::First => values[0],
        AggFunc::Last => *values.last().unwrap(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_agg_func() {
        let vals = vec![1.0, 5.0, 3.0, 2.0, 4.0];
        assert_eq!(apply_agg_func(&vals, AggFunc::Min), 1.0);
        assert_eq!(apply_agg_func(&vals, AggFunc::Max), 5.0);
        assert_eq!(apply_agg_func(&vals, AggFunc::Avg), 3.0);
        assert_eq!(apply_agg_func(&vals, AggFunc::First), 1.0);
        assert_eq!(apply_agg_func(&vals, AggFunc::Last), 4.0);
    }

    #[test]
    fn test_aggregate_empty() {
        let result = aggregate_by_period(&[], 86400, AggFunc::Avg);
        assert!(result.is_empty());
    }
}
