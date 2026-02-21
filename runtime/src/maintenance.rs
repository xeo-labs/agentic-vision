//! Autonomous runtime maintenance loop.
//!
//! Runs periodic cache cleanup and registry delta GC while the daemon is active.

use crate::collective::registry::LocalRegistry;
use crate::intelligence::cache::MapCache;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;

const DEFAULT_TICK_SECS: u64 = 300;
const DEFAULT_REGISTRY_GC_EVERY_TICKS: u32 = 12;
const DEFAULT_REGISTRY_DELTA_KEEP: usize = 120;
const DEFAULT_SLA_MAX_CACHE_ENTRIES_BEFORE_GC_THROTTLE: usize = 1200;
const DEFAULT_HEALTH_LEDGER_EMIT_SECS: u64 = 30;

#[derive(Debug, Clone, Copy)]
enum AutonomicProfile {
    Desktop,
    Cloud,
    Aggressive,
}

#[derive(Debug, Clone, Copy)]
struct ProfileDefaults {
    tick_secs: u64,
    registry_gc_every_ticks: u32,
    registry_delta_keep: usize,
    sla_max_cache_entries_before_gc_throttle: usize,
}

impl AutonomicProfile {
    fn from_env(name: &str) -> Self {
        let raw = read_env_string(name).unwrap_or_else(|| "desktop".to_string());
        match raw.trim().to_ascii_lowercase().as_str() {
            "cloud" => Self::Cloud,
            "aggressive" => Self::Aggressive,
            _ => Self::Desktop,
        }
    }

    fn defaults(self) -> ProfileDefaults {
        match self {
            Self::Desktop => ProfileDefaults {
                tick_secs: DEFAULT_TICK_SECS,
                registry_gc_every_ticks: DEFAULT_REGISTRY_GC_EVERY_TICKS,
                registry_delta_keep: DEFAULT_REGISTRY_DELTA_KEEP,
                sla_max_cache_entries_before_gc_throttle:
                    DEFAULT_SLA_MAX_CACHE_ENTRIES_BEFORE_GC_THROTTLE,
            },
            Self::Cloud => ProfileDefaults {
                tick_secs: 120,
                registry_gc_every_ticks: 6,
                registry_delta_keep: 240,
                sla_max_cache_entries_before_gc_throttle: 4000,
            },
            Self::Aggressive => ProfileDefaults {
                tick_secs: 60,
                registry_gc_every_ticks: 3,
                registry_delta_keep: 80,
                sla_max_cache_entries_before_gc_throttle: 800,
            },
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Desktop => "desktop",
            Self::Cloud => "cloud",
            Self::Aggressive => "aggressive",
        }
    }
}

#[derive(Debug, Clone)]
struct MaintenanceConfig {
    profile: AutonomicProfile,
    tick_every: Duration,
    registry_gc_every_ticks: u32,
    registry_delta_keep: usize,
    sla_max_cache_entries_before_gc_throttle: usize,
    health_ledger_emit_every: Duration,
}

impl MaintenanceConfig {
    fn from_env() -> Self {
        let profile = AutonomicProfile::from_env("CORTEX_AUTONOMIC_PROFILE");
        let defaults = profile.defaults();
        Self {
            profile,
            tick_every: Duration::from_secs(read_env_u64(
                "CORTEX_MAINTENANCE_TICK_SECS",
                defaults.tick_secs,
            )),
            registry_gc_every_ticks: read_env_u32(
                "CORTEX_REGISTRY_GC_EVERY_TICKS",
                defaults.registry_gc_every_ticks,
            )
            .max(1),
            registry_delta_keep: read_env_usize(
                "CORTEX_REGISTRY_GC_KEEP_DELTAS",
                defaults.registry_delta_keep,
            )
            .max(1),
            sla_max_cache_entries_before_gc_throttle: read_env_usize(
                "CORTEX_SLA_MAX_CACHE_ENTRIES_BEFORE_GC_THROTTLE",
                defaults.sla_max_cache_entries_before_gc_throttle,
            )
            .max(1),
            health_ledger_emit_every: Duration::from_secs(
                read_env_u64(
                    "CORTEX_HEALTH_LEDGER_EMIT_SECS",
                    DEFAULT_HEALTH_LEDGER_EMIT_SECS,
                )
                .max(5),
            ),
        }
    }
}

/// Spawn background maintenance until daemon shutdown is signaled.
pub fn spawn(shutdown: Arc<Notify>) -> tokio::task::JoinHandle<()> {
    let cfg = MaintenanceConfig::from_env();
    tokio::spawn(async move {
        tracing::info!(
            "maintenance loop started: profile={} tick={}s gc_every={} keep_deltas={} gc_throttle_cache_limit={}",
            cfg.profile.as_str(),
            cfg.tick_every.as_secs(),
            cfg.registry_gc_every_ticks,
            cfg.registry_delta_keep,
            cfg.sla_max_cache_entries_before_gc_throttle
        );
        let mut ticker = tokio::time::interval(cfg.tick_every);
        let mut tick_count: u64 = 0;
        let mut last_health_emit = std::time::Instant::now()
            .checked_sub(cfg.health_ledger_emit_every)
            .unwrap_or_else(std::time::Instant::now);
        let mut throttle_count: u64 = 0;

        loop {
            tokio::select! {
                _ = shutdown.notified() => {
                    tracing::info!("maintenance loop stopping");
                    break;
                }
                _ = ticker.tick() => {
                    tick_count = tick_count.saturating_add(1);
                    let cleanup = run_cache_cleanup();
                    let mut mode = "normal";
                    let mut gc_removed: Option<usize> = None;
                    if tick_count % cfg.registry_gc_every_ticks as u64 == 0 {
                        if cleanup.cache_entries_before > cfg.sla_max_cache_entries_before_gc_throttle {
                            mode = "throttled";
                            throttle_count = throttle_count.saturating_add(1);
                            tracing::debug!(
                                "maintenance throttled registry gc: cache_entries={} threshold={}",
                                cleanup.cache_entries_before,
                                cfg.sla_max_cache_entries_before_gc_throttle
                            );
                        } else {
                            gc_removed = run_registry_gc(cfg.registry_delta_keep);
                        }
                    }
                    if last_health_emit.elapsed() >= cfg.health_ledger_emit_every {
                        if let Err(e) = emit_health_ledger(
                            &cfg,
                            tick_count,
                            throttle_count,
                            mode,
                            &cleanup,
                            gc_removed,
                        ) {
                            tracing::warn!("maintenance health ledger emit failed: {e}");
                        } else {
                            last_health_emit = std::time::Instant::now();
                        }
                    }
                }
            }
        }
    })
}

#[derive(Debug, Clone)]
struct CacheCleanupResult {
    cache_entries_before: usize,
    cache_entries_after: usize,
    expired_removed: usize,
}

fn run_cache_cleanup() -> CacheCleanupResult {
    match MapCache::default_cache() {
        Ok(mut cache) => {
            let before = cache.len();
            cache.cleanup_expired();
            let after = cache.len();
            if after < before {
                tracing::info!(
                    "maintenance cache cleanup removed {} expired map(s)",
                    before - after
                );
            }
            CacheCleanupResult {
                cache_entries_before: before,
                cache_entries_after: after,
                expired_removed: before.saturating_sub(after),
            }
        }
        Err(e) => {
            tracing::warn!("maintenance cache cleanup failed to open cache: {e}");
            CacheCleanupResult {
                cache_entries_before: 0,
                cache_entries_after: 0,
                expired_removed: 0,
            }
        }
    }
}

fn run_registry_gc(keep_deltas: usize) -> Option<usize> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let registry_dir = home.join(".cortex").join("registry");
    match LocalRegistry::new(registry_dir) {
        Ok(mut registry) => match registry.gc(keep_deltas) {
            Ok(removed) if removed > 0 => {
                tracing::info!("maintenance registry gc removed {removed} delta file(s)");
                Some(removed)
            }
            Ok(_) => Some(0),
            Err(e) => {
                tracing::warn!("maintenance registry gc failed: {e}");
                None
            }
        },
        Err(e) => {
            tracing::warn!("maintenance registry open failed: {e}");
            None
        }
    }
}

fn emit_health_ledger(
    cfg: &MaintenanceConfig,
    tick_count: u64,
    throttle_count: u64,
    maintenance_mode: &str,
    cleanup: &CacheCleanupResult,
    gc_removed: Option<usize>,
) -> anyhow::Result<()> {
    let dir = resolve_health_ledger_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("agentic-vision.json");
    let tmp = dir.join("agentic-vision.json.tmp");
    let payload = serde_json::json!({
        "project": "AgenticVision",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "status": "ok",
        "autonomic": {
            "profile": cfg.profile.as_str(),
            "maintenance_mode": maintenance_mode,
            "tick_secs": cfg.tick_every.as_secs(),
            "registry_gc_every_ticks": cfg.registry_gc_every_ticks,
            "registry_delta_keep": cfg.registry_delta_keep,
            "throttle_count": throttle_count,
        },
        "sla": {
            "cache_entries_before_gc": cleanup.cache_entries_before,
            "max_cache_entries_before_gc_throttle": cfg.sla_max_cache_entries_before_gc_throttle,
        },
        "runtime": {
            "tick_count": tick_count,
            "cache_entries_before": cleanup.cache_entries_before,
            "cache_entries_after": cleanup.cache_entries_after,
            "cache_expired_removed": cleanup.expired_removed,
            "registry_gc_removed": gc_removed.unwrap_or(0),
        }
    });
    std::fs::write(&tmp, serde_json::to_vec_pretty(&payload)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

fn resolve_health_ledger_dir() -> PathBuf {
    if let Some(custom) = read_env_string("CORTEX_HEALTH_LEDGER_DIR") {
        if !custom.is_empty() {
            return PathBuf::from(custom);
        }
    }
    if let Some(custom) = read_env_string("AGENTRA_HEALTH_LEDGER_DIR") {
        if !custom.is_empty() {
            return PathBuf::from(custom);
        }
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".agentra")
        .join("health-ledger")
}

fn read_env_u64(name: &str, default_value: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default_value)
}

fn read_env_u32(name: &str, default_value: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(default_value)
}

fn read_env_usize(name: &str, default_value: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default_value)
}

fn read_env_string(name: &str) -> Option<String> {
    std::env::var(name).ok().map(|v| v.trim().to_string())
}
