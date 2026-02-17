// Copyright 2026 Cortex Contributors
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code, unused_imports)]

use anyhow::Result;
use clap::{Parser, Subcommand};

mod cartography;
mod cli;
mod extraction;
mod intelligence;
mod live;
mod map;
mod navigation;
mod pool;
mod protocol;
mod renderer;
mod server;
mod stealth;

#[derive(Parser)]
#[command(
    name = "cortex",
    about = "Rapid web cartographer for AI agents",
    version,
    long_about = "Cortex maps entire websites into navigable binary graphs.\nAgents navigate the map, not the site."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the Cortex daemon process
    Start,
    /// Stop the running Cortex daemon
    Stop,
    /// Check environment readiness (Chromium, memory, socket)
    Doctor,
    /// Show status of the running Cortex daemon
    Status,
    /// Map an entire website into a navigable graph
    Map {
        /// Domain to map (e.g. "amazon.com")
        domain: String,
        /// Maximum number of nodes in the map
        #[arg(long, default_value = "50000")]
        max_nodes: u32,
        /// Maximum number of pages to render
        #[arg(long, default_value = "200")]
        max_render: u32,
        /// Time budget in milliseconds
        #[arg(long, default_value = "10000")]
        timeout: u64,
    },
    /// Query a mapped site for matching pages
    Query {
        /// Domain to query (must be previously mapped)
        domain: String,
        /// Filter by page type (e.g. "product", "article")
        #[arg(long, name = "type")]
        page_type: Option<String>,
        /// Filter by price less than
        #[arg(long)]
        price_lt: Option<f32>,
        /// Filter by rating greater than
        #[arg(long)]
        rating_gt: Option<f32>,
        /// Maximum number of results
        #[arg(long, default_value = "10")]
        limit: u32,
    },
    /// Find shortest path between two nodes in a map
    Pathfind {
        /// Domain to pathfind in
        domain: String,
        /// Source node index
        #[arg(long)]
        from: u32,
        /// Target node index
        #[arg(long)]
        to: u32,
    },
    /// Perceive a single live page
    Perceive {
        /// URL to perceive
        url: String,
        /// Output format
        #[arg(long, default_value = "pretty")]
        format: String,
    },
    /// Install Chromium for Testing
    Install,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start => cli::start::run().await,
        Commands::Stop => cli::stop::run().await,
        Commands::Doctor => cli::doctor::run().await,
        Commands::Status => cli::status::run().await,
        Commands::Map {
            domain,
            max_nodes,
            max_render,
            timeout,
        } => cli::map_cmd::run(&domain, max_nodes, max_render, timeout).await,
        Commands::Query {
            domain,
            page_type,
            price_lt,
            rating_gt,
            limit,
        } => cli::query_cmd::run(&domain, page_type.as_deref(), price_lt, rating_gt, limit).await,
        Commands::Pathfind { domain, from, to } => {
            cli::pathfind_cmd::run(&domain, from, to).await
        }
        Commands::Perceive { url, format } => cli::perceive_cmd::run(&url, &format).await,
        Commands::Install => cli::install_cmd::run().await,
    }
}
