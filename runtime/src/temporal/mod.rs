//! Temporal Intelligence — history, pattern detection, prediction, and alerts.
//!
//! The temporal layer sits on top of the registry's delta history, exposing
//! time-series queries, statistical pattern detection, and watch/alert rules.

pub mod patterns;
pub mod query;
pub mod store;
pub mod watch;
