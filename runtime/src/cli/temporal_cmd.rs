//! CLI handlers for temporal commands (history, patterns).

use crate::cli::output;
use crate::collective::registry::LocalRegistry;
use crate::temporal::patterns;
use crate::temporal::store::TemporalStore;
use anyhow::Result;
use chrono::{DateTime, NaiveDateTime, Utc};
use std::path::PathBuf;
use std::sync::Arc;

fn registry_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".cortex").join("registry")
}

fn dim_name_to_num(name: &str) -> u8 {
    match name {
        "price" => 48,
        "original_price" => 49,
        "discount" => 50,
        "availability" => 51,
        "rating" => 52,
        "review_count" => 53,
        _ => name.parse().unwrap_or(0),
    }
}

/// Run the history command.
pub async fn run_history(domain: &str, url: &str, dim: &str, since: &str) -> Result<()> {
    let registry = Arc::new(LocalRegistry::new(registry_dir())?);
    let store = TemporalStore::new(registry);

    let since_dt: DateTime<Utc> = if let Ok(dt) = DateTime::parse_from_rfc3339(since) {
        dt.with_timezone(&Utc)
    } else if let Ok(date) = chrono::NaiveDate::parse_from_str(since, "%Y-%m-%d") {
        date.and_hms_opt(0, 0, 0).unwrap().and_utc()
    } else {
        anyhow::bail!(
            "invalid date format: {since}. Use ISO 8601 (e.g., 2025-01-01 or 2025-01-01T00:00:00Z)"
        );
    };

    let dim_num = dim_name_to_num(dim);
    let points = store.history(domain, url, dim_num, since_dt)?;

    if output::is_json() {
        let json_points: Vec<serde_json::Value> = points
            .iter()
            .map(|(ts, v)| serde_json::json!([ts.to_rfc3339(), v]))
            .collect();
        output::print_json(&serde_json::json!({"points": json_points}));
    } else if points.is_empty() {
        println!("  No history data found.");
    } else {
        println!("  History for {domain} {url} dim={dim} since {since}:\n");
        for (ts, val) in &points {
            println!("    {}  {:.2}", ts.format("%Y-%m-%d %H:%M"), val);
        }
    }

    Ok(())
}

/// Run the patterns command.
pub async fn run_patterns(domain: &str, url: &str, dim: &str) -> Result<()> {
    let registry = Arc::new(LocalRegistry::new(registry_dir())?);
    let store = TemporalStore::new(registry);

    let dim_num = dim_name_to_num(dim);
    let since = Utc::now() - chrono::Duration::days(365);
    let points = store.history(domain, url, dim_num, since)?;

    if points.is_empty() {
        if !output::is_quiet() {
            println!("  No history data for pattern detection.");
        }
        return Ok(());
    }

    let detected = patterns::detect_patterns(&points);

    if output::is_json() {
        output::print_json(&serde_json::json!({"patterns": detected}));
    } else if detected.is_empty() {
        println!("  No patterns detected (need more data points).");
    } else {
        println!("  Detected patterns:\n");
        for p in &detected {
            match p {
                patterns::Pattern::Trend {
                    direction,
                    slope,
                    confidence,
                } => {
                    println!(
                        "    Trend: {:?} (slope={:.3}/day, confidence={:.2})",
                        direction, slope, confidence
                    );
                }
                patterns::Pattern::Periodic {
                    period,
                    confidence,
                    phase,
                } => {
                    let days = *period as f64 / 86400.0;
                    println!(
                        "    Periodic: {:.1} day cycle (phase={:.2}, confidence={:.2})",
                        days, phase, confidence
                    );
                }
                patterns::Pattern::Anomaly {
                    timestamp,
                    expected_value,
                    actual_value,
                    sigma,
                } => {
                    println!(
                        "    Anomaly: at {} (expected={:.2}, actual={:.2}, {:.1}σ)",
                        timestamp.format("%Y-%m-%d"),
                        expected_value,
                        actual_value,
                        sigma
                    );
                }
                patterns::Pattern::Seasonal {
                    season,
                    discount_pct,
                    confidence,
                    ..
                } => {
                    println!(
                        "    Seasonal: {} ({:.0}% discount, confidence={:.2})",
                        season,
                        discount_pct * 100.0,
                        confidence
                    );
                }
            }
        }
    }

    Ok(())
}
