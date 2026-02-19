//! AgenticVision MCP Server — entry point.

use std::sync::Arc;
use tokio::sync::Mutex;

use clap::{Parser, Subcommand};

use agentic_vision_mcp::config::resolve_vision_path;
use agentic_vision_mcp::protocol::ProtocolHandler;
use agentic_vision_mcp::session::VisionSessionManager;
use agentic_vision_mcp::tools::ToolRegistry;
use agentic_vision_mcp::transport::StdioTransport;

#[derive(Parser)]
#[command(
    name = "agentic-vision-mcp",
    about = "MCP server for AgenticVision — universal LLM access to persistent visual memory",
    version
)]
struct Cli {
    /// Path to .avis vision file.
    #[arg(short, long)]
    vision: Option<String>,

    /// Path to CLIP ONNX model.
    #[arg(long)]
    model: Option<String>,

    /// Log level (trace, debug, info, warn, error).
    #[arg(long, default_value = "info")]
    log_level: String,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start MCP server over stdio (default).
    Serve {
        /// Path to .avis vision file.
        #[arg(short, long)]
        vision: Option<String>,

        /// Path to CLIP ONNX model.
        #[arg(long)]
        model: Option<String>,

        /// Log level (trace, debug, info, warn, error).
        #[arg(long)]
        log_level: Option<String>,
    },

    /// Validate a .avis vision file.
    Validate,

    /// Print server capabilities as JSON.
    Info,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&cli.log_level));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .init();

    match cli.command.unwrap_or(Commands::Serve {
        vision: None,
        model: None,
        log_level: None,
    }) {
        Commands::Serve {
            vision,
            model,
            log_level: _,
        } => {
            let effective_vision = vision.or(cli.vision);
            let effective_model = model.or(cli.model);
            let vision_path = resolve_vision_path(effective_vision.as_deref());
            let session =
                VisionSessionManager::open(&vision_path, effective_model.as_deref())?;
            let session = Arc::new(Mutex::new(session));
            let handler = ProtocolHandler::new(session);
            let transport = StdioTransport::new(handler);
            transport.run().await?;
        }

        Commands::Validate => {
            let vision_path = resolve_vision_path(cli.vision.as_deref());
            match VisionSessionManager::open(&vision_path, None) {
                Ok(session) => {
                    let store = session.store();
                    println!("Valid vision file: {vision_path}");
                    println!("  Captures: {}", store.count());
                    println!("  Embedding dim: {}", store.embedding_dim);
                    println!("  Sessions: {}", store.session_count);
                }
                Err(e) => {
                    eprintln!("Invalid vision file: {e}");
                    std::process::exit(1);
                }
            }
        }

        Commands::Info => {
            let capabilities = agentic_vision_mcp::types::InitializeResult::default_result();
            let tools = ToolRegistry::list_tools();
            let info = serde_json::json!({
                "server": capabilities.server_info,
                "protocol_version": capabilities.protocol_version,
                "capabilities": capabilities.capabilities,
                "tools": tools.iter().map(|t| &t.name).collect::<Vec<_>>(),
                "tool_count": tools.len(),
            });
            println!("{}", serde_json::to_string_pretty(&info)?);
        }
    }

    Ok(())
}
