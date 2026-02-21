//! Interactive REPL for the AgenticVision MCP server.
//!
//! Launch with `agentic-vision-mcp repl` to enter interactive mode.
//! Type `/help` for available commands, Tab for completion.

use rustyline::completion::{Completer, Pair};
use rustyline::config::CompletionType;
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{
    Cmd, ConditionalEventHandler, Config, Editor, Event, EventContext, EventHandler, Helper,
    KeyEvent, RepeatCount,
};

use crate::config::resolve_vision_path;
use crate::session::VisionSessionManager;
use crate::tools::ToolRegistry;

/// Available REPL commands.
const COMMANDS: &[(&str, &str)] = &[
    ("/info", "Show server capabilities and tools"),
    ("/validate", "Validate a .avis vision file"),
    ("/tools", "List available MCP tools"),
    ("/load", "Load a .avis file"),
    ("/stats", "Show capture statistics"),
    ("/clear", "Clear the screen"),
    ("/help", "Show available commands"),
    ("/exit", "Quit the REPL"),
];

/// REPL helper for tab completion.
struct VisionHelper;

impl Default for VisionHelper {
    fn default() -> Self {
        Self
    }
}

impl Completer for VisionHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &rustyline::Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let input = &line[..pos];

        if !input.contains(' ') {
            let matches: Vec<Pair> = COMMANDS
                .iter()
                .filter(|(cmd, _)| cmd.starts_with(input))
                .map(|(cmd, desc)| Pair {
                    display: format!("{cmd:<16} {desc}"),
                    replacement: format!("{cmd} "),
                })
                .collect();
            return Ok((0, matches));
        }

        // .avis file completion
        let parts: Vec<&str> = input.splitn(2, ' ').collect();
        let cmd = parts[0];
        let args = if parts.len() > 1 { parts[1] } else { "" };

        if cmd == "/load" || cmd == "/validate" {
            let mut files = Vec::new();
            if let Ok(entries) = std::fs::read_dir(".") {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().is_some_and(|e| e == "avis") {
                        if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                            files.push(name.to_string());
                        }
                    }
                }
            }
            files.sort();
            let prefix_start = input.len() - args.len();
            let matches: Vec<Pair> = files
                .iter()
                .filter(|f| f.starts_with(args.trim()))
                .map(|f| Pair {
                    display: f.clone(),
                    replacement: format!("{f} "),
                })
                .collect();
            return Ok((prefix_start, matches));
        }

        Ok((pos, Vec::new()))
    }
}

impl Hinter for VisionHelper {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, _ctx: &rustyline::Context<'_>) -> Option<String> {
        if pos < line.len() || line.is_empty() {
            return None;
        }
        if line.starts_with('/') && !line.contains(' ') {
            for (cmd, _) in COMMANDS {
                if cmd.starts_with(line) && *cmd != line {
                    return Some(cmd[line.len()..].to_string());
                }
            }
        }
        None
    }
}

impl Highlighter for VisionHelper {}
impl Validator for VisionHelper {}
impl Helper for VisionHelper {}

struct TabCompleteOrAcceptHint;

impl ConditionalEventHandler for TabCompleteOrAcceptHint {
    fn handle(
        &self,
        _evt: &Event,
        _n: RepeatCount,
        _positive: bool,
        ctx: &EventContext<'_>,
    ) -> Option<Cmd> {
        if ctx.has_hint() {
            Some(Cmd::CompleteHint)
        } else {
            Some(Cmd::Complete)
        }
    }
}

/// Session state.
struct ReplState {
    vision_path: Option<String>,
}

/// Run the interactive REPL.
pub fn run() -> anyhow::Result<()> {
    eprintln!();
    eprintln!(
        "  \x1b[32m\u{25c9}\x1b[0m \x1b[1magentic-vision-mcp v{}\x1b[0m \x1b[90m\u{2014} Visual Memory for AI Agents\x1b[0m",
        env!("CARGO_PKG_VERSION")
    );
    eprintln!();
    eprintln!(
        "    Press \x1b[36m/\x1b[0m to browse commands, \x1b[90mTab\x1b[0m to complete, \x1b[90m/exit\x1b[0m to quit."
    );
    eprintln!();

    let config = Config::builder()
        .history_ignore_space(true)
        .auto_add_history(true)
        .completion_type(CompletionType::List)
        .completion_prompt_limit(20)
        .build();

    let mut rl: Editor<VisionHelper, rustyline::history::DefaultHistory> =
        Editor::with_config(config)?;
    rl.set_helper(Some(VisionHelper));
    rl.bind_sequence(
        KeyEvent::from('\t'),
        EventHandler::Conditional(Box::new(TabCompleteOrAcceptHint)),
    );

    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    let hist_path = std::path::PathBuf::from(&home).join(".agentic_vision_mcp_history");
    if hist_path.exists() {
        let _ = rl.load_history(&hist_path);
    }

    let mut state = ReplState { vision_path: None };
    let prompt = " \x1b[36mvision>\x1b[0m ";

    loop {
        match rl.readline(prompt) {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                let input = line.strip_prefix('/').unwrap_or(line);
                if input.is_empty() {
                    cmd_help();
                    continue;
                }

                let mut parts = input.splitn(2, ' ');
                let cmd = parts.next().unwrap_or("");
                let args = parts.next().unwrap_or("").trim();

                match cmd {
                    "exit" | "quit" => {
                        eprintln!("  \x1b[90m\u{2728}\x1b[0m Goodbye!");
                        break;
                    }
                    "help" | "h" | "?" => cmd_help(),
                    "clear" | "cls" => eprint!("\x1b[2J\x1b[H"),
                    "info" => cmd_info(),
                    "tools" => cmd_tools(),
                    "validate" => cmd_validate(args, &state),
                    "load" => cmd_load(args, &mut state),
                    "stats" => cmd_stats(&state),
                    _ => {
                        eprintln!("  Unknown command '/{cmd}'. Type /help for commands.");
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                eprintln!("  \x1b[90m(Ctrl+C)\x1b[0m Type \x1b[1m/exit\x1b[0m to quit.");
            }
            Err(ReadlineError::Eof) => {
                eprintln!("  \x1b[90m\u{2728}\x1b[0m Goodbye!");
                break;
            }
            Err(err) => {
                eprintln!("  Error: {err}");
                break;
            }
        }
    }

    let _ = std::fs::create_dir_all(hist_path.parent().unwrap_or(std::path::Path::new(".")));
    let _ = rl.save_history(&hist_path);

    Ok(())
}

fn cmd_help() {
    eprintln!();
    eprintln!("  Commands:");
    eprintln!();
    for (cmd, desc) in COMMANDS {
        eprintln!("    {cmd:<18} {desc}");
    }
    eprintln!();
    eprintln!("  Tip: Tab completion works for commands and .avis files.");
    eprintln!();
}

fn cmd_info() {
    let capabilities = crate::types::InitializeResult::default_result();
    let tools = ToolRegistry::list_tools();
    eprintln!();
    eprintln!(
        "  Server:   {} v{}",
        capabilities.server_info.name, capabilities.server_info.version
    );
    eprintln!("  Protocol: {}", capabilities.protocol_version);
    eprintln!("  Tools:    {}", tools.len());
    eprintln!();
}

fn cmd_tools() {
    let tools = ToolRegistry::list_tools();
    eprintln!();
    eprintln!("  {} MCP tools available:", tools.len());
    eprintln!();
    for tool in &tools {
        eprintln!(
            "    {:<28} {}",
            tool.name,
            tool.description.as_deref().unwrap_or("")
        );
    }
    eprintln!();
}

fn cmd_validate(args: &str, state: &ReplState) {
    let path = if args.is_empty() {
        match &state.vision_path {
            Some(p) => p.clone(),
            None => resolve_vision_path(None),
        }
    } else {
        args.split_whitespace().next().unwrap_or(args).to_string()
    };

    match VisionSessionManager::open(&path, None) {
        Ok(session) => {
            let store = session.store();
            eprintln!();
            eprintln!("  Valid vision file: {path}");
            eprintln!("    Captures:      {}", store.count());
            eprintln!("    Embedding dim: {}", store.embedding_dim);
            eprintln!("    Sessions:      {}", store.session_count);
            eprintln!();
        }
        Err(e) => {
            eprintln!("  Invalid vision file: {e}");
        }
    }
}

fn cmd_load(args: &str, state: &mut ReplState) {
    if args.is_empty() {
        eprintln!("  Usage: /load <file.avis>");
        return;
    }
    let path = args.split_whitespace().next().unwrap_or(args).to_string();
    match VisionSessionManager::open(&path, None) {
        Ok(session) => {
            let store = session.store();
            eprintln!(
                "  Loaded: {path} ({} captures, dim {})",
                store.count(),
                store.embedding_dim
            );
            state.vision_path = Some(path);
        }
        Err(e) => {
            eprintln!("  Failed to load: {e}");
        }
    }
}

fn cmd_stats(state: &ReplState) {
    let path = match &state.vision_path {
        Some(p) => p.clone(),
        None => resolve_vision_path(None),
    };

    match VisionSessionManager::open(&path, None) {
        Ok(session) => {
            let store = session.store();
            eprintln!();
            eprintln!("  Vision store: {path}");
            eprintln!("    Captures:      {}", store.count());
            eprintln!("    Embedding dim: {}", store.embedding_dim);
            eprintln!("    Sessions:      {}", store.session_count);
            eprintln!();
        }
        Err(e) => {
            eprintln!("  Cannot read vision store: {e}");
        }
    }
}
