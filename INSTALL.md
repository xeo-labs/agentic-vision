# Installation Guide

## Requirements

- **OS:** macOS (arm64/x86_64), Linux (x86_64/aarch64)
- **Rust:** 1.75+ (for `cargo install`)
- **Node.js:** 18+ (for TypeScript client / MCP server)
- **Python:** 3.9+ (for Python client)

## Quick Install

### One-liner (macOS / Linux)

```bash
curl -fsSL https://cortex.dev/install | bash
```

This downloads the pre-built binary, adds it to your `PATH`, and runs `cortex doctor` to verify the installation.

### From source (Rust)

```bash
cargo install cortex-runtime
```

Compiles from crates.io. Requires Rust 1.75+ and a C linker.

### Homebrew (macOS)

```bash
brew install cortex-ai/cortex/cortex
```

### npm (wrapper)

```bash
npm install -g @cortex/cli
```

Installs a thin npm wrapper that downloads the appropriate platform binary on first use.

### Docker

```bash
# Lite image — HTTP-only, no Chromium (~25 MB)
docker run -p 7700:7700 cortex-ai/cortex:lite

# Full image — includes Chromium for fallback rendering (~350 MB)
docker run -p 7700:7700 cortex-ai/cortex:full
```

## Client Libraries

### Python

```bash
pip install cortex-agent
```

Requires the Cortex runtime to be installed and running. The daemon starts automatically on first use.

### TypeScript / Node.js

```bash
npm install @cortex-ai/client
```

Requires the Cortex runtime to be installed and running.

## Agent Integration

### Auto-discover and connect all agents

```bash
cortex plug
```

This auto-detects AI agents on your system (Claude Desktop, Claude Code, Cursor, Windsurf, Continue, Cline) and configures them to use Cortex via MCP. Safe and idempotent — run it as many times as you like.

### Manual MCP configuration

If `cortex plug` doesn't detect your agent, add this to your agent's MCP config:

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

### Check status

```bash
cortex plug --status   # See which agents are connected
cortex plug --remove   # Clean removal from all agents
cortex plug --list     # List detected agent config files
```

## Verify Installation

```bash
cortex doctor
```

This checks:
- Runtime binary is on PATH
- Daemon can start
- Socket connection works
- Chromium availability (optional)
- Network connectivity

## First Run

```bash
# Map your first site
cortex map example.com

# Check what was mapped
cortex query example.com

# Full workflow
cortex map amazon.com && cortex compile amazon.com
cortex wql "SELECT url, page_type FROM Node LIMIT 5"
```

The daemon starts automatically. Chromium is downloaded on-demand only if a page requires browser rendering (rare — 93% of sites work via HTTP).

## Environment Variables

| Variable | Default | Description |
|:---------|:--------|:------------|
| `CORTEX_SOCKET` | `/tmp/cortex.sock` | Unix socket path |
| `CORTEX_DATA_DIR` | `~/.cortex` | Data directory for maps and cache |
| `CORTEX_LOG_LEVEL` | `info` | Log verbosity (trace, debug, info, warn, error) |
| `CORTEX_MAX_NODES` | `50000` | Default max nodes per map |
| `CORTEX_TIMEOUT_MS` | `30000` | Default mapping timeout |
| `CORTEX_HTTP_PORT` | (disabled) | REST API port (e.g. `7700`) |
| `CORTEX_CHROMIUM_PATH` | (auto-detected) | Custom Chromium binary path |

## Troubleshooting

### "Cortex daemon not running"

```bash
cortex start          # Start the daemon manually
cortex doctor         # Diagnose the issue
```

### "Connection refused" on socket

The socket file may be stale. Remove it and restart:

```bash
rm /tmp/cortex.sock
cortex restart
```

### Chromium installation fails

Cortex downloads Chromium for Testing on-demand. If this fails behind a proxy:

```bash
cortex install                    # Manual Chromium download
CORTEX_CHROMIUM_PATH=/path/to/chromium cortex start   # Use existing browser
```

Most sites (93%) don't need Chromium at all. Use `cortex map --no-browser` to skip browser fallback entirely.

### macOS: "cortex" can't be opened because Apple cannot check it for malicious software

```bash
xattr -d com.apple.quarantine $(which cortex)
```

### Permission denied on socket

```bash
chmod 755 /tmp/cortex.sock
# Or use a user-local socket:
CORTEX_SOCKET=~/.cortex/cortex.sock cortex start
```

## Uninstall

```bash
# Remove the binary
cargo uninstall cortex-runtime

# Remove agent integrations
cortex plug --remove

# Remove data
rm -rf ~/.cortex

# Remove socket
rm -f /tmp/cortex.sock
```

## Platform Notes

### macOS (Apple Silicon)
Native arm64 binary. Best performance. All benchmarks in the documentation were measured on Apple M4 Pro.

### macOS (Intel)
x86_64 binary via Rosetta 2 or native compilation. Performance within 2x of Apple Silicon.

### Linux (x86_64)
Native compilation. Requires glibc 2.31+ or musl for static builds.

### Linux (aarch64)
Native compilation on ARM64 servers (AWS Graviton, Ampere Altra). Full feature parity.

### Windows
Not natively supported. Use WSL2 or Docker:
```bash
# WSL2
wsl --install
# Then follow Linux instructions inside WSL

# Docker
docker run -p 7700:7700 cortex-ai/cortex:lite
```
