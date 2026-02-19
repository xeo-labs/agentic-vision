// Copyright 2026 Cortex Contributors
// SPDX-License-Identifier: Apache-2.0

//! Cortex runtime library — rapid web cartographer for AI agents.
//!
//! This library crate exposes the core modules for integration testing.

#![allow(
    dead_code,
    unused_imports,
    clippy::new_without_default,
    clippy::should_implement_trait
)]

pub mod acquisition;
pub mod audit;
pub mod cartography;
pub mod cli;
pub mod collective;
pub mod compiler;
pub mod events;
pub mod extraction;
pub mod intelligence;
pub mod live;
pub mod map;
pub mod navigation;
pub mod pool;
pub mod progress;
pub mod protocol;
pub mod renderer;
pub mod rest;
pub mod server;
pub mod stealth;
pub mod temporal;
pub mod trust;
pub mod wql;
