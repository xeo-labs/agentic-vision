# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-02-19

First stable release. All packages at v1.0.0. 397 tests passing. 100-site benchmark: 85.3/100 average.

### Added

**Core Runtime**
- Binary SiteMap format (`.ctx`) — 128-dimensional feature vectors, ~600 bytes/node, 2.7x compression vs JSON, CRC32 integrity checks
- 6-layer HTTP-first acquisition engine: robots.txt/sitemap discovery → JSON-LD/OpenGraph extraction → CSS-selector pattern engine → platform API discovery → browser fallback
- Page classification: 16 page types (Home, ProductListing, SearchResults, ProductDetail, Cart, Article, Documentation, Login, Checkout, Profile, ApiEndpoint, Media, Form, Dashboard, Error, Other)
- Navigation engine: filter by type/features, Dijkstra pathfinding, cosine similarity search, k-nearest neighbors, graph clustering
- Web Compiler: schema inference from binary maps, 5-target code generation (Python, TypeScript, OpenAPI, GraphQL, MCP)
- WQL (Web Query Language): SQL-like syntax for web data (`SELECT name, price FROM Product WHERE price < 200 ORDER BY price ASC`)
- Collective Graph: delta-based map sharing, registry with push/pull/gc, privacy stripping (zeroes session dimensions 112-127)
- Temporal Intelligence: history store, trend detection, anomaly detection, periodicity analysis, value predictions
- Live interaction: perceive, act (HTTP-first), watch, browser sessions (fallback)
- Trust layer: credential vault, PII detection, content sanitization
- Audit logging: JSONL event log with optional remote sync
- Background daemon with Unix socket and REST API (Axum, 11 endpoints)
- Server-Sent Events for real-time mapping progress
- Web Dashboard at `localhost:7700/dashboard`
- Interactive REPL with 15 slash commands, tab completion, ghost text hints
- CLI with 17 commands and global flags (`--json`, `--quiet`, `--verbose`, `--no-color`)
- Shell completions for bash, zsh, and fish
- Docker images: lite (~25 MB, HTTP-only) and full (~350 MB, with Chromium)

**Agent Integration**
- `cortex plug` command: auto-discovers Claude Desktop, Claude Code, Cursor, Windsurf, Continue, and Cline — injects 9 MCP tools with safe, idempotent configuration
- MCP server (`@cortex/mcp-server`): 9 tools — map, query, pathfind, act, perceive, compare, auth, compile, wql
- REST API: 11 endpoints on configurable HTTP port
- Framework adapters: LangChain, CrewAI, AutoGen, Semantic Kernel, OpenClaw

**Clients**
- Python client (`cortex-client`): map, filter, pathfind, similar, perceive, compare, login, WQL
- TypeScript client (`@cortex-ai/client`): map, filter, pathfind, similar, perceive, status

**Documentation**
- Research paper: 10 pages, 8 figures, 13 tables, 22 references (all real benchmark data)
- SVG architecture, benchmark, and comparison diagrams
- 11 runnable example scripts
- Comprehensive docs/ directory with quickstart, concepts, API reference, benchmarks, integration guide, FAQ
- INSTALL.md with platform-specific notes and troubleshooting
- LIMITATIONS.md with 17 documented v1.0 constraints

### Performance (Apple M4 Pro, 64 GB, Rust 1.90.0 --release)

| Operation | Latency |
|:---|---:|
| Filter query (10K nodes) | <1 µs |
| WQL full pipeline (10K nodes) | 86 µs |
| Pathfind (10K nodes) | 884 µs |
| Serialize 10K nodes | 134.9 ms |
| Similarity top-10 (10K nodes) | 42.7 ms |
| Schema inference (10K nodes) | 96.9 ms |
| Map a site (HTTP-first) | 3-15 s |

---

## [0.4.5] - 2026-02-18

### Live Visibility — Event Bus, Streaming Progress, Web Dashboard

- **Event Bus** (`events.rs`): 17 typed `CortexEvent` variants covering mapping, actions, auth, queries, and system lifecycle
- **Progress Infrastructure** (`progress.rs`): Granular mapper telemetry with layer-by-layer progress events
- **Server-Sent Events**: `GET /api/v1/events` with optional `?domain=` filtering
- **Web Dashboard**: `http://localhost:7700/dashboard` — live activity feed, progress bars, connected agents
- **Enhanced Status**: Rich `/api/v1/status` with per-map details
- **CLI Streaming**: `cortex map` shows live layer-by-layer progress

## [0.4.4] - 2026-02-18

### Interactive REPL — Claude Code-Style Terminal UI

- Interactive mode via `cortex` with no subcommand
- 15 slash commands, tab completion, ghost text hints, persistent history
- Animated progress bars during site mapping

## [0.4.3] - 2026-02-18

### Phase 7E — Public Release Completion

- 11 runnable example scripts
- README overhaul with installation methods and framework integrations
- Research paper (LaTeX) with 100-site benchmark data

## [0.4.2] - 2026-02-18

- `cortex plug --config-dir` flag for testing
- `--http-port` flag for `cortex start`
- Gateway test suite, plug test suite, 100-site test harness v3
- Python client: `act()` and `compare()` functions

## [0.4.1] - 2026-02-18

### Gateway Layer

- MCP server with 7 tools (map, query, pathfind, act, perceive, compare, auth)
- REST API on configurable port
- OpenAPI specification
- Framework adapters: LangChain, CrewAI, OpenClaw
- `cortex plug` command for agent auto-discovery

## [0.4.0] - 2026-02-18

- WebMCP integration for high-reliability action execution
- WebMCP tool discovery

## [0.3.0] - 2026-02-18

- Drag-and-drop discovery via semantic API replay
- Canvas/WebGL state extraction via accessibility trees
- HTTP-native OAuth flow
- Native WebSocket client

## [0.2.1] - 2026-02-18

- Expanded CSS selector coverage for prices, ratings, availability
- Magento and BigCommerce platform detection
- 100-site test suite v2

## [0.2.0] - 2026-02-18

### Layered Acquisition Engine (No-Browser Mapping)

- HTTP-first site mapping (sitemap.xml → JSON-LD → CSS selectors → API discovery → browser fallback)
- Structured data extraction (JSON-LD, OpenGraph, meta tags)
- Pattern engine with CSS selector database
- HTTP action discovery and execution
- Platform action templates (Shopify, WooCommerce, Magento, BigCommerce)
- HTTP session management and authentication

## [0.1.0] - 2026-02-17

Initial release.

- SiteMap binary format (CTX) with 128-dim feature vectors
- Cartography engine with Chromium rendering
- Navigation engine (query, pathfind, similarity, clustering)
- Live interaction (perceive, act, watch, sessions)
- Intelligence layer (caching, progressive rendering, smart sampling)
- Trust & safety (credential vault, PII detection, sanitization)
- Python and TypeScript clients
- LangChain, CrewAI, OpenClaw adapters
- CLI with 9 commands
- GitHub Actions CI/CD

[1.0.0]: https://github.com/cortex-ai/cortex/releases/tag/v1.0.0
[0.4.5]: https://github.com/cortex-ai/cortex/releases/tag/v0.4.5
[0.4.4]: https://github.com/cortex-ai/cortex/releases/tag/v0.4.4
[0.4.3]: https://github.com/cortex-ai/cortex/releases/tag/v0.4.3
[0.4.2]: https://github.com/cortex-ai/cortex/releases/tag/v0.4.2
[0.4.1]: https://github.com/cortex-ai/cortex/releases/tag/v0.4.1
[0.4.0]: https://github.com/cortex-ai/cortex/releases/tag/v0.4.0
[0.3.0]: https://github.com/cortex-ai/cortex/releases/tag/v0.3.0
[0.2.1]: https://github.com/cortex-ai/cortex/releases/tag/v0.2.1
[0.2.0]: https://github.com/cortex-ai/cortex/releases/tag/v0.2.0
[0.1.0]: https://github.com/cortex-ai/cortex/releases/tag/v0.1.0
