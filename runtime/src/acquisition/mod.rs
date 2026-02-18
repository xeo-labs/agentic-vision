//! Layered HTTP-based acquisition engine.
//!
//! Replaces browser-dependent mapping with structured data extraction
//! from raw HTML. The browser becomes a last-resort fallback for mapping
//! and remains required only for ACT and live PERCEIVE.

pub mod action_discovery;
pub mod api_discovery;
pub mod feed_parser;
pub mod head_scanner;
pub mod http_client;
pub mod js_analyzer;
pub mod pattern_engine;
pub mod structured;
