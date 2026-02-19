# @cortex/mcp-server

[Model Context Protocol](https://modelcontextprotocol.io/) server for [Cortex](https://github.com/cortex-ai/cortex) — gives AI agents web cartography superpowers.

## What This Does

When connected to your AI agent, Cortex adds 9 tools that let the agent **map entire websites into graphs** and query them in microseconds — instead of browsing page by page.

| Tool | What it does |
|------|-------------|
| `cortex_map` | Map a website into a navigable graph (3-15 seconds) |
| `cortex_query` | Search the map by page type, price, rating, etc. |
| `cortex_pathfind` | Find shortest path between two pages |
| `cortex_act` | Execute actions (add to cart, search, submit form) |
| `cortex_perceive` | Analyze a single page in detail |
| `cortex_compare` | Compare products/pages across multiple sites |
| `cortex_auth` | Authenticate to access protected content |
| `cortex_compile` | Generate typed API clients from a mapped site |
| `cortex_wql` | Run SQL-like queries across mapped domains |

## Setup (One Command)

```bash
# Auto-detect your AI agents and connect Cortex
cortex plug
```

This detects and configures:
- **Claude Desktop** — adds to `claude_desktop_config.json`
- **Claude Code** — adds to `.claude/settings.json`
- **Cursor** — adds to `.cursor/mcp.json`
- **Windsurf** — adds to Codeium config
- **Continue** — adds to `.continue/config.json`
- **Cline** — adds to VS Code global storage

After running, restart your agent (if prompted) and try:

> "Map github.com and tell me what types of pages it has"

## Manual Setup

If you prefer manual configuration, add to your agent's MCP config:

```json
{
  "mcpServers": {
    "cortex": {
      "command": "npx",
      "args": ["-y", "@cortex/mcp-server"]
    }
  }
}
```

## Manage

```bash
cortex plug --status    # See which agents are connected
cortex plug --remove    # Disconnect Cortex from all agents
cortex plug --list      # List detected agents
```

## Requirements

- **Cortex runtime** must be installed: `cargo install cortex-runtime`
- **Node.js** >= 18 (for `npx`)
- The Cortex daemon starts automatically on first tool use

## How It Works

```
Your Agent (Claude, Cursor, etc.)
    | MCP protocol (stdio)
@cortex/mcp-server (this package)
    | Unix socket / JSON protocol
Cortex Runtime (Rust daemon)
    | HTTP-first extraction
Websites
```

The agent never touches a browser. Cortex maps sites via HTTP (sitemaps, JSON-LD, structured data) and serves results from an in-memory graph.

## License

Apache-2.0
