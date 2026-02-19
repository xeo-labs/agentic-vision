//! Collective Web Graph — local registry and remote sync for sharing maps.
//!
//! Maps are stored locally in a registry with delta-based incremental updates.
//! Optional remote sync enables sharing across Cortex instances.

pub mod delta;
pub mod registry;
pub mod sync;
