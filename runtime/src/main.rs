// Copyright 2026 Cortex Contributors
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code, unused_imports)]

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;

mod audit;
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
mod trust;

#[derive(Parser)]
#[command(
    name = "cortex",
    about = "Cortex — Rapid web cartographer for AI agents",
    version,
    after_help = "Run 'cortex <command> --help' for details on each command."
)]
struct Cli {
    /// Output results as JSON (machine-readable)
    #[arg(long, global = true)]
    json: bool,

    /// Suppress non-essential output
    #[arg(long, short, global = true)]
    quiet: bool,

    /// Enable verbose/debug logging
    #[arg(long, short, global = true)]
    verbose: bool,

    /// Disable colored output
    #[arg(long, global = true)]
    no_color: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the Cortex background process
    Start,
    /// Stop the Cortex background process
    Stop,
    /// Restart the Cortex background process
    Restart,
    /// Check environment and diagnose issues
    Doctor,
    /// Show runtime status and cached maps
    Status,
    /// Map a website into a navigable binary graph
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
        /// Force re-mapping even if a cached map exists
        #[arg(long)]
        fresh: bool,
    },
    /// Search a mapped site by type, features, or similarity
    Query {
        /// Domain to query (must be previously mapped)
        domain: String,
        /// Filter by page type (e.g. "product_detail", "article", "0x04")
        #[arg(long, name = "type")]
        page_type: Option<String>,
        /// Filter by price less than (shorthand for --feature "48<VALUE")
        #[arg(long)]
        price_lt: Option<f32>,
        /// Filter by rating greater than (shorthand for --feature "52>VALUE")
        #[arg(long)]
        rating_gt: Option<f32>,
        /// Feature range filter (e.g. "48<300", "52>0.8"). Can be repeated.
        #[arg(long = "feature")]
        feature_filters: Vec<String>,
        /// Maximum number of results
        #[arg(long, default_value = "20")]
        limit: u32,
    },
    /// Find shortest path between pages on a mapped site
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
        /// Output format (pretty, json, vector)
        #[arg(long, default_value = "pretty")]
        format: String,
    },
    /// Download and install Chromium
    Install {
        /// Force reinstall even if Chromium is already installed
        #[arg(long)]
        force: bool,
    },
    /// Manage cached maps
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },
    /// Generate shell completion scripts
    Completions {
        /// Shell type (bash, zsh, fish, powershell)
        shell: Shell,
    },
}

#[derive(Subcommand)]
enum CacheAction {
    /// Clear cached maps (all or for a specific domain)
    Clear {
        /// Domain to clear (omit to clear all)
        domain: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Set global flags via environment variables so all modules can check them
    if cli.json {
        std::env::set_var("CORTEX_JSON", "1");
    }
    if cli.quiet {
        std::env::set_var("CORTEX_QUIET", "1");
    }
    if cli.verbose {
        std::env::set_var("CORTEX_VERBOSE", "1");
    }
    if cli.no_color {
        std::env::set_var("CORTEX_NO_COLOR", "1");
    }

    let result = match cli.command {
        Commands::Start => cli::start::run().await,
        Commands::Stop => cli::stop::run().await,
        Commands::Restart => cli::restart_cmd::run().await,
        Commands::Doctor => cli::doctor::run().await,
        Commands::Status => cli::status::run().await,
        Commands::Map {
            domain,
            max_nodes,
            max_render,
            timeout,
            fresh,
        } => cli::map_cmd::run(&domain, max_nodes, max_render, timeout, fresh).await,
        Commands::Query {
            domain,
            page_type,
            price_lt,
            rating_gt,
            feature_filters,
            limit,
        } => {
            cli::query_cmd::run(
                &domain,
                page_type.as_deref(),
                price_lt,
                rating_gt,
                limit,
                &feature_filters,
            )
            .await
        }
        Commands::Pathfind { domain, from, to } => {
            cli::pathfind_cmd::run(&domain, from, to).await
        }
        Commands::Perceive { url, format } => cli::perceive_cmd::run(&url, &format).await,
        Commands::Install { force } => cli::install_cmd::run_with_force(force).await,
        Commands::Cache { action } => match action {
            CacheAction::Clear { domain } => {
                cli::cache_cmd::run_clear(domain.as_deref()).await
            }
        },
        Commands::Completions { shell } => {
            let mut cmd = Cli::command();
            clap_complete::generate(shell, &mut cmd, "cortex", &mut std::io::stdout());
            Ok(())
        }
    };

    // Consistent exit codes: 0=success, 1=error
    if let Err(e) = &result {
        if !cli::output::is_quiet() && !cli::output::is_json() {
            eprintln!("  Error: {e:#}");
        }
        if cli::output::is_json() {
            cli::output::print_json(&serde_json::json!({
                "error": true,
                "message": format!("{e:#}"),
            }));
        }
        std::process::exit(1);
    }

    result
}
