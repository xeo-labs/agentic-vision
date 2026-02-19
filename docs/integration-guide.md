# Integration Guide

Cortex integrates with AI agents via MCP (Model Context Protocol), REST API, language clients, and framework adapters.

## MCP — Recommended for AI Agents

MCP is the fastest way to give any AI agent web cartography capabilities. One command connects Cortex to all supported agents.

### Auto-Setup

```bash
cortex plug
```

This auto-discovers AI agents on your machine and injects 9 Cortex tools:

| Agent | Config File |
|:------|:-----------|
| Claude Desktop | `~/Library/Application Support/Claude/claude_desktop_config.json` |
| Claude Code | `~/.claude/settings.json` |
| Cursor | `~/.cursor/mcp.json` |
| Windsurf | `~/.codeium/windsurf/mcp_config.json` |
| Continue | `~/.continue/config.json` |
| Cline | `~/.cline/mcp_settings.json` |

The injection is **safe and idempotent** — run `cortex plug` as many times as you want. It adds the Cortex MCP server entry without modifying existing configuration.

### What Gets Injected

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

### Available MCP Tools

| Tool | Description | Required Params |
|:-----|:------------|:---------------|
| `cortex_map` | Map a website into a binary graph | `domain` |
| `cortex_query` | Search by page type and features | `domain` |
| `cortex_pathfind` | Shortest path between pages | `domain`, `from_node`, `to_node` |
| `cortex_act` | Execute an action (add-to-cart, search, etc.) | `domain`, `node`, `action` |
| `cortex_perceive` | Live state of a single page | `url` |
| `cortex_compare` | Compare across multiple sites | `domains` |
| `cortex_auth` | Authenticate with a site | `domain`, `method` |
| `cortex_compile` | Compile schema and generate clients | `domain` |
| `cortex_wql` | Execute a WQL query | `query` |

### Managing MCP Integration

```bash
cortex plug --status     # Check which agents are connected
cortex plug --list       # List all detected config files
cortex plug --remove     # Clean removal from all agents
```

### Manual MCP Setup

If `cortex plug` doesn't detect your agent, add the MCP server entry to your agent's config file manually:

**Claude Desktop** — Edit `~/Library/Application Support/Claude/claude_desktop_config.json`:
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

**Cursor** — Edit `~/.cursor/mcp.json`:
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

---

## REST API

Start the daemon with HTTP enabled:

```bash
cortex start --http-port 7700
```

Then use any HTTP client:

```bash
# Map a site
curl -X POST http://localhost:7700/api/v1/map \
  -H "Content-Type: application/json" \
  -d '{"domain": "amazon.com"}'

# Query products
curl -X POST http://localhost:7700/api/v1/query \
  -H "Content-Type: application/json" \
  -d '{"domain": "amazon.com", "page_type": 4, "features": {"48": {"lt": 300}}, "limit": 10}'

# Execute WQL
curl -X POST http://localhost:7700/api/v1/wql \
  -H "Content-Type: application/json" \
  -d '{"query": "SELECT name, price FROM Product WHERE price < 200 LIMIT 10"}'
```

### Server-Sent Events

Stream real-time mapping progress:

```bash
curl http://localhost:7700/api/v1/events
curl http://localhost:7700/api/v1/events?domain=amazon.com   # Filter by domain
```

### Web Dashboard

Visit `http://localhost:7700/dashboard` for a real-time web dashboard showing:
- Active mapping operations with layer-by-layer progress
- Connected agents
- Cached maps
- Session statistics

---

## Framework Adapters

### LangChain

```bash
pip install cortex-langchain
```

```python
from cortex_langchain import CortexMapTool, CortexQueryTool, CortexPathfindTool

tools = [CortexMapTool(), CortexQueryTool(), CortexPathfindTool()]

# Use with LangChain agent
from langchain.agents import create_tool_calling_agent
agent = create_tool_calling_agent(llm, tools, prompt)
```

### CrewAI

```bash
pip install cortex-crewai
```

```python
from cortex_crewai import CortexTool

tool = CortexTool()

# Use with CrewAI agent
from crewai import Agent
agent = Agent(
    role="Web Researcher",
    tools=[tool],
    goal="Research product prices across e-commerce sites"
)
```

### AutoGen

```bash
pip install cortex-autogen
```

```python
from cortex_autogen import CortexSkill

skill = CortexSkill()

# Register with AutoGen agent
agent.register_skill(skill)
```

### Semantic Kernel

```bash
pip install cortex-semantic-kernel
```

```python
from cortex_semantic_kernel import CortexPlugin

kernel.add_plugin(CortexPlugin(), "cortex")
```

### OpenClaw

```bash
pip install cortex-openclaw
```

```python
from cortex_openclaw import cortex_map_skill, cortex_query_skill

# Register as OpenClaw skills
agent.add_skill(cortex_map_skill)
agent.add_skill(cortex_query_skill)
```

---

## Docker

### Lite Image (HTTP-only)

```bash
docker run -p 7700:7700 cortex-ai/cortex:lite
```

~25 MB. No Chromium. Suitable for 93% of sites.

### Full Image (with Chromium)

```bash
docker run -p 7700:7700 cortex-ai/cortex:full
```

~350 MB. Includes Chromium for browser fallback.

### Docker Compose

```yaml
version: "3.8"
services:
  cortex:
    image: cortex-ai/cortex:lite
    ports:
      - "7700:7700"
    volumes:
      - cortex-data:/data
    environment:
      - CORTEX_HTTP_PORT=7700
      - CORTEX_DATA_DIR=/data

volumes:
  cortex-data:
```

### Using Docker with MCP

Point your agent's MCP config at the Docker container:

```json
{
  "mcpServers": {
    "cortex": {
      "command": "docker",
      "args": ["exec", "-i", "cortex", "cortex-mcp-server"]
    }
  }
}
```

---

## Unix Socket Protocol

For custom clients, connect to the Unix socket at `/tmp/cortex.sock` (configurable via `CORTEX_SOCKET`).

The protocol uses JSON-RPC 2.0 over Unix domain sockets:

```json
{"jsonrpc": "2.0", "method": "map", "params": {"domain": "example.com"}, "id": 1}
```

Response:
```json
{"jsonrpc": "2.0", "result": {"domain": "example.com", "node_count": 15, "edge_count": 22}, "id": 1}
```

Available methods: `map`, `query`, `pathfind`, `perceive`, `act`, `auth`, `compile`, `wql`, `status`.
