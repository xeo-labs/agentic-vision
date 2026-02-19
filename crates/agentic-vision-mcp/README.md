# AgenticVision-MCP

**MCP server for AgenticVision — universal LLM access to persistent visual memory.**

[![crates.io](https://img.shields.io/crates/v/agentic-vision-mcp.svg)](https://crates.io/crates/agentic-vision-mcp)
[![MIT License](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

## What it does

AgenticVision-MCP exposes the [AgenticVision](https://crates.io/crates/agentic-vision) engine over the [Model Context Protocol](https://modelcontextprotocol.io) (JSON-RPC 2.0 over stdio). Any MCP-compatible LLM gains persistent visual memory — capture screenshots, embed with CLIP ViT-B/32, compare, recall.

## Install

```bash
cargo install agentic-vision-mcp
```

## Configure Claude Desktop

Add to `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "vision": {
      "command": "agentic-vision-mcp",
      "args": ["--vision", "~/.vision.avis", "serve"]
    }
  }
}
```

## Configure Claude Code

Add to `~/.claude/mcp.json`:

```json
{
  "mcpServers": {
    "vision": {
      "command": "agentic-vision-mcp",
      "args": ["--vision", "~/.vision.avis", "serve"]
    }
  }
}
```

## Configure VS Code / Cursor

Add to `.vscode/settings.json`:

```json
{
  "mcp.servers": {
    "vision": {
      "command": "agentic-vision-mcp",
      "args": ["--vision", "${workspaceFolder}/.vision/project.avis", "serve"]
    }
  }
}
```

## Configure Windsurf

Add to `~/.codeium/windsurf/mcp_config.json`:

```json
{
  "mcpServers": {
    "vision": {
      "command": "agentic-vision-mcp",
      "args": ["--vision", "~/.vision.avis", "serve"]
    }
  }
}
```

> **Do not use `/tmp` for vision files** — macOS and Linux clear this directory periodically. Use `~/.vision.avis` for persistent storage.

## MCP Surface Area

| Category | Count | Examples |
|:---|---:|:---|
| **Tools** | 10 | `vision_capture`, `vision_similar`, `vision_diff`, `vision_compare`, `vision_query`, `vision_ocr`, `vision_track`, `vision_link`, `session_start`, `session_end` |
| **Resources** | 6 | `avis://capture/{id}`, `avis://session/{id}`, `avis://timeline/{start}/{end}`, `avis://similar/{id}`, `avis://stats`, `avis://recent` |
| **Prompts** | 4 | `observe`, `compare`, `track`, `describe` |

## How it works

1. **Capture** — `vision_capture` accepts images from files, base64, screenshots, or the system clipboard. Embeds with CLIP ViT-B/32, stores in `.avis` binary format. Screenshots support optional region capture on macOS and Linux.
2. **Query** — `vision_query` retrieves by time, description, or recency. `vision_similar` finds visually similar captures by cosine similarity.
3. **Compare** — `vision_compare` for side-by-side LLM analysis. `vision_diff` for pixel-level differencing with 8×8 grid region detection.
4. **Link** — `vision_link` connects captures to [AgenticMemory](https://github.com/xeo-labs/agentic-memory) cognitive graph nodes.

## CLI Commands

```bash
# Start server (stdio) — defaults to ~/.vision.avis
agentic-vision-mcp serve

# Start server with custom vision file and model
agentic-vision-mcp --vision /path/to/file.avis --model /path/to/clip.onnx serve

# Validate a vision file
agentic-vision-mcp --vision ~/.vision.avis validate

# Print server info as JSON
agentic-vision-mcp info
```

## Performance

| Operation | Time |
|:---|---:|
| MCP tool round-trip | **7.2 ms** |
| Image capture | **47 ms** |
| Similarity search (top-5) | **1-2 ms** |
| Visual diff | **<1 ms** |

## Development

This crate is part of the [AgenticVision](../../README.md) Cargo workspace.

```bash
# Run MCP server tests (from workspace root)
cargo test -p agentic-vision-mcp

# Run all workspace tests
cargo test --workspace

# Clippy + format
cargo clippy --workspace
cargo fmt --all

# Build release
cargo build --release
```

## Protocol

This server implements MCP (Model Context Protocol) spec version **2024-11-05** over JSON-RPC 2.0. Transport: **stdio** (newline-delimited JSON over stdin/stdout).

## Links

- [GitHub](https://github.com/xeo-labs/agentic-vision)
- [Core Library](https://crates.io/crates/agentic-vision)
- [AgenticMemory](https://github.com/xeo-labs/agentic-memory) — Persistent cognitive memory for AI agents

## License

MIT
