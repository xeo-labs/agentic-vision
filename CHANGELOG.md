# Changelog

## v0.4.4 (2026-02-18)

### Interactive REPL — Claude Code-Style Terminal UI

- **Interactive mode**: Run `cortex` with no subcommand to enter the interactive REPL
- **15 slash commands**: `/map`, `/query`, `/pathfind`, `/perceive`, `/status`, `/doctor`, `/maps`, `/use`, `/settings`, `/cache`, `/plug`, `/help`, `/clear`, `/exit`
- **Tab completion**: Complete commands, cached domains, page types with Tab key
- **Ghost text hints**: See command suggestions as you type
- **Command history**: Persistent history at `~/.cortex/repl_history`
- **Animated progress**: `indicatif` multi-progress bars during site mapping
- **Active domain**: After `/map example.com`, subsequent queries auto-target that domain
- **Welcome banner**: Shows daemon status, cached maps count, and version on startup
- **Dependencies**: `rustyline` 14 (line editing), `indicatif` 0.17 (progress bars)

## v0.4.3 (2026-02-18)

### Phase 7E — Public Release Completion

- **Examples directory**: 11 runnable example scripts covering quickstart, price comparison, pathfinding, cross-site comparison, page perception, and all 5 framework integrations (LangChain, CrewAI, AutoGen, Semantic Kernel, MCP)
- **README overhaul**: Added installation methods (curl, Homebrew, npm, cargo), agent auto-setup section, Docker section, expanded framework table (AutoGen, Semantic Kernel, MCP), updated project structure
- **Research paper**: Publication-grade LaTeX paper in `publication/` with 8 figures, 8 tables, 22 references, 100-site benchmark data

## v0.4.2 (2026-02-18)

### Improvements

- **CLI**: `cortex plug --config-dir` flag for testing against custom configuration directories
- **REST API**: `--http-port` flag for `cortex start` to enable the HTTP REST API
- **Testing**: Gateway test suite, plug test suite, 100-site test harness v3
- **Python client**: Added top-level `act()` and `compare()` functions

## v0.4.1 (2026-02-18)

### Major Changes — Gateway Layer

- **MCP server** (`integrations/mcp-server/`): MCP tools for `cortex_map`, `cortex_query`, `cortex_pathfind`, `cortex_act`, `cortex_perceive`, `cortex_compare`, `cortex_auth`
- **REST API** (`runtime/src/rest.rs`): HTTP endpoints for all protocol methods on configurable port
- **OpenAPI specification** (`integrations/openapi.yaml`): Full API schema
- **Framework adapters**: LangChain, CrewAI, OpenClaw adapters for agent integration
- **`cortex plug` command**: Auto-discover AI agents, inject MCP server config, remove, status check

## v0.4.0 (2026-02-18)

### Major Changes — WebMCP Integration

- **WebMCP integration** (`acquisition/webmcp.rs`): Highest-reliability action execution via `navigator.modelContext`
- **WebMCP tool discovery**: Automatic detection of WebMCP-enabled sites

## v0.3.0 (2026-02-18)

### Major Changes — Advanced Actions

- **Drag-and-drop discovery** via semantic API replay (`drag_discovery.rs`, `drag_platforms.json`)
- **Canvas/WebGL state extraction** via accessibility trees and internal APIs (`canvas_extractor.rs`, `known_canvas_apis.json`)
- **HTTP-native OAuth flow** in `auth.rs`
- **Native WebSocket client** (`ws_discovery.rs`, `ws_platforms.json`, `live/websocket.rs`)

## v0.2.1 (2026-02-18)

### Improvements

- **Pattern engine**: Expanded CSS selector coverage in `css_selectors.json` for broader price, rating, and availability extraction
- **Platform actions**: Added detection patterns for Magento and BigCommerce in `platform_actions.json`
- **HTTP action execution**: Broader form detection across site types
- **Test suite v2**: 100-site test suite with data source quality and action discovery scoring

## v0.2.0 (2026-02-18)

### Major Changes — Layered Acquisition Engine (No-Browser Mapping)

- **HTTP-first site mapping**: Sites are mapped via layered HTTP acquisition instead of browser rendering. Sitemap.xml, robots.txt, HEAD scanning, and feed discovery (Layer 0), then HTTP GET with JSON-LD/OpenGraph/meta tag parsing (Layer 1), pattern engine CSS selectors (Layer 1.5), API discovery for known platforms (Layer 2), and action discovery (Layer 2.5). Browser rendering is Layer 3 fallback only.
- **Structured data extraction** (`acquisition/structured.rs`): JSON-LD, OpenGraph, meta tags, headings, forms, and links extracted from raw HTML without rendering
- **Pattern engine** (`acquisition/pattern_engine.rs`): CSS selector database (`css_selectors.json`) for extracting prices, ratings, availability from HTML when structured data is sparse
- **HTTP action discovery** (`acquisition/action_discovery.rs`): Discovers forms, JS endpoints, and platform-specific actions. Many actions (add-to-cart, search) executable via HTTP POST
- **Platform action templates** (`acquisition/platform_actions.json`): Pre-built action definitions for Shopify, WooCommerce, Magento, BigCommerce
- **HTTP session & authentication** (`acquisition/http_session.rs`, `acquisition/auth.rs`): Password login, OAuth, API key authentication — password login works without a browser
- **JS analyzer** (`acquisition/js_analyzer.rs`): Extracts API endpoints and platform indicators from JavaScript sources
- **Deleted**: `crawler.rs`, `sampler.rs`, `interpolator.rs`, `smart_sampler.rs` — replaced by acquisition engine

## v0.1.0 (2026-02-17)

Initial release of Cortex — the rapid web cartographer for AI agents.

### Features

- **SiteMap binary format (CTX)**: 128-dimension feature vectors, PageType classification, OpCode action encoding, CSR edge structure, k-means clustering
- **Cartography engine**: Robots.txt parsing, sitemap discovery, URL classification, page rendering with Chromium, feature encoding, action encoding, sample-based mapping with interpolation
- **Navigation engine**: Query by page type / feature ranges / flags, Dijkstra pathfinding, cosine similarity nearest-neighbor search, k-means clustering
- **Live interaction**: Perceive (single page encoding), Refresh (re-render nodes), Act (execute browser actions), Watch (monitor changes), persistent browser sessions
- **Intelligence layer**: Map caching with TTL, progressive rendering queue, smart sampling, cross-site query merging
- **Trust & safety**: Credential vault (SQLite), PII detection (email, phone, SSN, credit card), input sanitization (XSS, SQL injection, path traversal)
- **Audit logging**: JSONL append-only log, optional remote ledger sync
- **Stealth**: Browser fingerprint patching, human-like timing delays
- **Python client**: Zero-dependency stdlib-only client with auto-start, SiteMap wrapper, full type annotations
- **TypeScript client**: Unix socket client with full type exports
- **Framework integrations**: LangChain tools, CrewAI tool, OpenClaw skills
- **CLI**: `start`, `stop`, `doctor`, `status`, `map`, `query`, `pathfind`, `perceive`, `install` commands
- **CI/CD**: GitHub Actions for Rust/Python/TypeScript, multi-platform release builds
