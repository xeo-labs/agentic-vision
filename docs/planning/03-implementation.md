# 03 — Implementation Specification

> Exact dependencies, file-by-file build order, function signatures, and test requirements.

## 1. Rust Dependencies

**`runtime/Cargo.toml`:**

```toml
[package]
name = "cortex-runtime"
version = "0.1.0"
edition = "2021"
license = "Apache-2.0"
description = "Rapid web cartographer for AI agents"

[[bin]]
name = "cortex"
path = "src/main.rs"

[dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "2.0"
anyhow = "1.0"
chromiumoxide = { version = "0.7", features = ["tokio-runtime"] }
futures = "0.3"
rusqlite = { version = "0.32", features = ["bundled"] }
clap = { version = "4", features = ["derive"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
uuid = { version = "1", features = ["v4"] }
reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
dirs = "6"
chrono = { version = "0.4", features = ["serde"] }
async-trait = "0.1"
byteorder = "1.5"
memmap2 = "0.9"
quick-xml = "0.37"
url = "2.5"
regex = "1.11"
fnv = "1.0"
rayon = "1.10"
dashmap = "6"
petgraph = "0.6"
rand = "0.8"
base64 = "0.22"

[dev-dependencies]
tempfile = "3"
tokio-test = "0.4"
assert_json_diff = "2.0"
criterion = "0.5"
wiremock = "0.6"
```

## 2. TypeScript Extractor Dependencies

**`extractors/package.json`:**

```json
{
  "name": "@cortex-ai/extractors",
  "version": "0.1.0",
  "private": true,
  "license": "Apache-2.0",
  "scripts": {
    "build": "bash build.sh",
    "lint": "eslint 'core/**/*.ts' 'shared/**/*.ts'",
    "test": "vitest run"
  },
  "devDependencies": {
    "typescript": "^5.7.0",
    "esbuild": "^0.24.0",
    "eslint": "^9.0.0",
    "@typescript-eslint/eslint-plugin": "^8.0.0",
    "@typescript-eslint/parser": "^8.0.0",
    "vitest": "^2.1.0",
    "jsdom": "^25.0.0"
  }
}
```

## 3. Python Client

**`clients/python/pyproject.toml`:**

```toml
[project]
name = "cortex-client"
version = "0.1.0"
description = "Thin client for the Cortex web cartography runtime"
license = { text = "Apache-2.0" }
requires-python = ">=3.10"
dependencies = []

[project.optional-dependencies]
dev = ["pytest>=8.0", "pytest-asyncio>=0.24", "ruff>=0.8", "mypy>=1.13"]

[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"
```

**Zero external dependencies.** Uses only stdlib: `socket`, `json`, `dataclasses`, `struct`, `subprocess`, `pathlib`, `time`, `os`.

## 4. File-by-File Build Order

### Phase 1: Foundation

```
# CLI shell
runtime/src/main.rs                      # clap CLI: map, query, pathfind, start, stop, restart, doctor, status, perceive, install, plug, cache, completions
runtime/src/cli/mod.rs                   # module declarations
runtime/src/cli/start.rs                 # daemonize, PID file, socket creation
runtime/src/cli/stop.rs                  # read PID, SIGTERM, cleanup
runtime/src/cli/doctor.rs               # env check: chromium, memory, socket, deps
runtime/src/cli/status.rs               # connect to socket, print status

# Chromium management
scripts/install-chromium.sh              # download Chrome for Testing per platform
runtime/src/renderer/mod.rs              # Renderer + RenderContext traits
runtime/src/renderer/chromium.rs         # launch, navigate, inject JS, extract, close

# Socket server
runtime/src/protocol.rs                  # parse JSON requests, route, format responses
runtime/src/server.rs                    # Unix domain socket listener, connection handler

# SiteMap types
runtime/src/map/mod.rs                   # module declarations
runtime/src/map/types.rs                 # SiteMap, NodeRecord, EdgeRecord, ActionRecord structs
runtime/src/map/builder.rs              # SiteMapBuilder: add nodes, edges, features, serialize
runtime/src/map/reader.rs               # read/query SiteMap: find nodes, filter, iterate
runtime/src/map/serializer.rs           # serialize SiteMap to binary format (CTX file)
runtime/src/map/deserializer.rs         # deserialize binary CTX file to SiteMap
```

### Phase 2: Cartography Engine

```
# Sitemap parsing
runtime/src/cartography/mod.rs           # module declarations
runtime/src/cartography/sitemap.rs       # parse sitemap.xml, sitemap index, RSS sitemaps
runtime/src/cartography/robots.rs        # parse robots.txt, extract crawl rules + sitemap URLs
runtime/src/cartography/url_classifier.rs # classify URLs by pattern → PageType

# Acquisition engine (HTTP-first data acquisition, replaces crawler/sampler/interpolator)
runtime/src/acquisition/mod.rs             # layered acquisition orchestrator
runtime/src/acquisition/http_client.rs      # async HTTP client (reqwest-based)
runtime/src/acquisition/structured.rs       # JSON-LD, OpenGraph, meta tags, links, forms parser
runtime/src/acquisition/head_scanner.rs     # parallel HEAD requests for metadata
runtime/src/acquisition/feed_parser.rs      # RSS/Atom feed parser
runtime/src/acquisition/api_discovery.rs    # detect and use public APIs
runtime/src/acquisition/pattern_engine.rs   # CSS selector + regex extraction for sparse data
runtime/src/acquisition/css_selectors.json  # CSS selector database (price, rating, availability)
runtime/src/acquisition/action_discovery.rs # HTML form + JS endpoint + platform action discovery
runtime/src/acquisition/js_analyzer.rs      # JavaScript source analysis for API endpoints
runtime/src/acquisition/platform_actions.json # platform-specific action templates
runtime/src/acquisition/http_session.rs     # HTTP session management with cookie jar
runtime/src/acquisition/auth.rs             # HTTP authentication (password, OAuth, API key)
runtime/src/acquisition/drag_discovery.rs    # Drag-and-drop action discovery via semantic API replay
runtime/src/acquisition/drag_platforms.json   # Platform-specific drag-drop API patterns
runtime/src/acquisition/canvas_extractor.rs  # Canvas/WebGL state extraction via accessibility tree and internal APIs
runtime/src/acquisition/known_canvas_apis.json # Known canvas app API patterns
runtime/src/acquisition/ws_discovery.rs      # WebSocket endpoint discovery and protocol detection
runtime/src/acquisition/ws_platforms.json    # Platform-specific WebSocket protocol patterns
runtime/src/acquisition/webmcp.rs            # WebMCP tool discovery and execution via navigator.modelContext

runtime/src/cartography/rate_limiter.rs  # respect crawl-delay, max concurrent per domain

# Feature extraction + encoding
runtime/src/cartography/feature_encoder.rs  # StructuredData + DOM extraction → 128-float feature vector
runtime/src/cartography/action_encoder.rs   # extracted actions → OpCode list
runtime/src/cartography/page_classifier.rs  # structured data + URL + metadata → PageType

# Map assembly
runtime/src/cartography/mapper.rs        # orchestrate: acquisition layers + encode → SiteMap
                                          # this is the MAP protocol handler's core logic

# Extraction scripts (TypeScript → compiled JS)
extractors/shared/dom-walker.ts
extractors/shared/visibility-checker.ts
extractors/shared/bbox-calculator.ts
extractors/core/content.ts               # headings, text, prices, tables, images, lists
extractors/core/actions.ts               # buttons, forms, inputs, selects → action list
extractors/core/navigation.ts            # all links, resolved URLs, classified types
extractors/core/structure.ts             # page structure: regions, layout, DOM depth
extractors/core/metadata.ts              # schema.org, JSON-LD, meta tags, OpenGraph
extractors/build.sh                      # compile all to JS bundles
runtime/src/extraction/mod.rs
runtime/src/extraction/loader.rs         # load compiled JS, inject into browser context
```

### Phase 3: Navigation Engine

```
runtime/src/navigation/mod.rs
runtime/src/navigation/query.rs          # QUERY handler: filter nodes by type, features, flags
runtime/src/navigation/pathfinder.rs     # PATHFIND handler: Dijkstra/A* on SiteMap graph
runtime/src/navigation/similarity.rs     # vector similarity search (brute force + KD-tree)
runtime/src/navigation/cluster.rs        # cluster detection: automatic site section grouping
```

### Phase 4: Thin Clients

```
# Python
clients/python/cortex_client/__init__.py      # public API
clients/python/cortex_client/connection.py    # socket client
clients/python/cortex_client/autostart.py     # auto-launch cortex process
clients/python/cortex_client/protocol.py      # method builders
clients/python/cortex_client/sitemap.py       # SiteMap class with query/pathfind helpers
clients/python/cortex_client/errors.py        # exception types

# TypeScript
clients/typescript/src/index.ts
clients/typescript/src/connection.ts
clients/typescript/src/sitemap.ts
clients/typescript/src/client.ts

# Conformance
clients/conformance/test_map.json
clients/conformance/test_query.json
clients/conformance/test_pathfind.json
clients/conformance/runner.py
```

### Phase 5: Live Interaction

```
runtime/src/live/mod.rs
runtime/src/live/perceive.rs             # PERCEIVE handler: render single page, return encoding + content
runtime/src/live/refresh.rs              # REFRESH handler: re-render nodes, update map
runtime/src/live/act.rs                  # ACT handler: execute opcode on live page
runtime/src/live/watch.rs                # WATCH handler: periodic refresh, stream deltas
runtime/src/live/session.rs              # session management for multi-step flows
runtime/src/live/websocket.rs            # native WebSocket session management for real-time apps

runtime/src/pool/mod.rs
runtime/src/pool/manager.rs              # browser pool: create/reuse/evict contexts
runtime/src/pool/context.rs              # browser context wrapper
runtime/src/pool/resource_governor.rs    # memory limits, context limits, timeouts
runtime/src/stealth/mod.rs
runtime/src/stealth/fingerprint.rs       # browser fingerprint patching
runtime/src/stealth/behavior.rs          # human-like delays
```

### Phase 6: Intelligence

```
runtime/src/intelligence/mod.rs
runtime/src/intelligence/progressive.rs      # background refinement after initial map delivery
runtime/src/intelligence/cache.rs            # map caching: don't re-map fresh sites
runtime/src/intelligence/cross_site.rs       # merge/compare maps from multiple domains
# Note: The acquisition engine and pattern engine replaced the former
# smart_sampler and interpolator modules as of v0.2.0
```

### Phase 7: Framework Integrations

```
integrations/langchain/cortex_langchain/tools.py
integrations/crewai/cortex_crewai/tools.py
integrations/openclaw/skills/cortex_map.py
integrations/openclaw/skills/cortex_navigate.py
integrations/openclaw/skill_manifest.json
integrations/mcp-server/src/index.ts        # MCP server for Claude, Cursor, Continue, Windsurf
integrations/mcp-server/src/cortex-client.ts # MCP server's Cortex socket client
integrations/openapi.yaml                   # OpenAPI 3.0 REST API specification
runtime/src/rest.rs                         # REST API endpoints (Axum)
runtime/src/cli/plug.rs                     # cortex plug: agent auto-discovery and MCP injection

runtime/src/cli/map_cmd.rs               # cortex map <domain> [--max-nodes N] [--render N]
runtime/src/cli/query_cmd.rs             # cortex query <domain> --type product --price-lt 300
runtime/src/cli/pathfind_cmd.rs          # cortex pathfind <domain> --from root --to <node>
runtime/src/cli/perceive_cmd.rs          # cortex perceive <url> [--format pretty|json]
```

### Phase 8: Hardening

```
runtime/src/trust/mod.rs
runtime/src/trust/credentials.rs         # encrypted credential vault
runtime/src/trust/pii.rs                 # PII detection in content
runtime/src/trust/sandbox.rs             # action input sanitization
runtime/src/audit/mod.rs
runtime/src/audit/logger.rs              # local JSONL audit log
runtime/src/audit/ledger_sync.rs         # optional AgentLedger integration
```

## 5. Key Function Signatures

### Mapper (core logic)

```rust
pub struct Mapper {
    pool: Arc<PoolManager>,
    extractor_loader: ExtractionLoader,
}

impl Mapper {
    /// Map an entire site. Returns a complete SiteMap.
    pub async fn map(&self, request: MapRequest) -> Result<SiteMap>;
    
    /// Internal: fetch and parse sitemap.xml
    async fn fetch_sitemap(&self, domain: &str) -> Result<Vec<SitemapEntry>>;
    
    /// Internal: classify URLs by pattern
    fn classify_urls(&self, urls: &[String]) -> Vec<(String, PageType, f32)>;
    
    /// Internal: run layered acquisition (HTTP GET → structured data → pattern engine → API discovery)
    async fn acquire_data(&self, urls: &[String]) -> Result<Vec<AcquiredPage>>;

    /// Internal: render pages in browser (Layer 3 fallback for low-data pages)
    async fn render_fallback(&self, urls: &[String]) -> Result<Vec<(NodeRecord, Vec<f32>, Vec<ActionRecord>, Vec<String>)>>;
    
    /// Internal: assemble the final SiteMap
    fn build_map(&self, domain: &str, nodes: Vec<NodeData>, edges: Vec<EdgeData>) -> SiteMap;
}
```

### SiteMap (data structure)

```rust
pub struct SiteMap {
    pub header: MapHeader,
    pub nodes: Vec<NodeRecord>,
    pub edges: Vec<EdgeRecord>,
    pub edge_index: Vec<u32>,
    pub features: Vec<[f32; 128]>,
    pub actions: Vec<ActionRecord>,
    pub action_index: Vec<u32>,
    pub cluster_assignments: Vec<u16>,
    pub cluster_centroids: Vec<[f32; 128]>,
    pub urls: Vec<String>,
}

impl SiteMap {
    /// Filter nodes by criteria
    pub fn filter(&self, query: &NodeQuery) -> Vec<NodeMatch>;
    
    /// Find k nearest nodes by feature vector similarity
    pub fn nearest(&self, target: &[f32; 128], k: usize) -> Vec<NodeMatch>;
    
    /// Shortest path between two nodes
    pub fn shortest_path(&self, from: u32, to: u32, constraints: &PathConstraints) -> Option<Path>;
    
    /// Get all edges from a node
    pub fn edges_from(&self, node: u32) -> &[EdgeRecord];
    
    /// Get feature vector for a node
    pub fn features(&self, node: u32) -> &[f32; 128];
    
    /// Serialize to binary CTX format
    pub fn serialize(&self) -> Vec<u8>;
    
    /// Deserialize from binary CTX format
    pub fn deserialize(data: &[u8]) -> Result<Self>;
    
    /// Update a node with fresh data
    pub fn update_node(&mut self, index: u32, record: NodeRecord, features: [f32; 128]);
}
```

### Python Client API

```python
# cortex_client/__init__.py

def map(domain: str, *, max_nodes: int = 50000, max_render: int = 200,
        max_time_ms: int = 10000, respect_robots: bool = True) -> SiteMap:
    """Map an entire website. Returns a navigable SiteMap."""

def map_many(domains: list[str], **kwargs) -> list[SiteMap]:
    """Map multiple websites in parallel."""

def perceive(url: str, *, include_content: bool = True) -> PageResult:
    """Perceive a single live page. For verification and text extraction."""

def perceive_many(urls: list[str], **kwargs) -> list[PageResult]:
    """Perceive multiple live pages in parallel."""

def status() -> RuntimeStatus:
    """Get Cortex runtime status."""

class SiteMap:
    """Navigable binary site map."""
    domain: str
    node_count: int
    edge_count: int
    
    def filter(self, *, page_type: int | list[int] | None = None,
               features: dict[int, dict] | None = None,
               flags: dict[str, bool] | None = None,
               sort_by: tuple[int, str] | None = None,
               limit: int = 100) -> list[NodeMatch]:
        """Filter nodes by type, features, and flags."""
    
    def nearest(self, goal_vector: list[float], k: int = 10) -> list[NodeMatch]:
        """Find k nearest nodes by feature similarity."""
    
    def pathfind(self, from_node: int, to_node: int, *,
                 avoid_flags: list[str] | None = None,
                 minimize: str = "hops") -> Path | None:
        """Find shortest path between nodes."""
    
    def refresh(self, *, nodes: list[int] | None = None,
                cluster: int | None = None,
                stale_threshold: float | None = None) -> RefreshResult:
        """Re-render specific nodes and update the map."""
    
    def act(self, node: int, opcode: tuple[int, int],
            params: dict | None = None,
            session_id: str | None = None) -> ActResult:
        """Execute an action on a live page."""
    
    def watch(self, *, nodes: list[int] | None = None,
              cluster: int | None = None,
              features: list[int] | None = None,
              interval_ms: int = 60000) -> Iterator[WatchDelta]:
        """Monitor nodes for changes."""
```

## 6. Testing Architecture

### Mapping Fixtures

```
tests/mapping-suite/
├── sites/
│   ├── simple-blog/               # ~20 pages: home, 10 posts, about, contact, archive
│   │   ├── index.html
│   │   ├── about.html
│   │   ├── post-1.html ... post-10.html
│   │   ├── archive.html
│   │   ├── contact.html
│   │   ├── sitemap.xml
│   │   └── robots.txt
│   ├── ecommerce-small/           # ~100 pages: home, categories, products, cart, checkout
│   │   ├── index.html
│   │   ├── category-electronics.html
│   │   ├── product-*.html (×50)
│   │   ├── cart.html
│   │   ├── checkout.html
│   │   ├── sitemap.xml
│   │   └── robots.txt
│   ├── spa-react/                 # SPA: single HTML + JS that renders multiple views
│   │   ├── index.html
│   │   └── app.js
│   └── no-sitemap/                # no sitemap.xml, must crawl to discover
│       ├── index.html
│       └── ... (pages linked from index)
├── golden/
│   ├── simple-blog.json           # expected SiteMap structure (node count, types, edges)
│   ├── ecommerce-small.json
│   ├── spa-react.json
│   └── no-sitemap.json
└── runner.rs                      # test runner: serve site, map it, compare to golden
```

### Golden File Format

```json
{
  "domain": "simple-blog.test",
  "expected_nodes": {"min": 15, "max": 25},
  "expected_edges": {"min": 30, "max": 60},
  "required_node_types": {
    "home": 1,
    "article": {"min": 8, "max": 12},
    "about_page": 1,
    "contact_page": 1
  },
  "required_paths": [
    {"from": "home", "to": "article", "max_hops": 2},
    {"from": "home", "to": "contact_page", "max_hops": 1}
  ],
  "required_features": {
    "articles_have_text_density_above": 0.3,
    "home_has_link_count_above": 5
  }
}
```

## 7. CI/CD Pipeline

**`.github/workflows/ci.yml`:**

```yaml
name: CI
on: [push, pull_request]
jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { components: "rustfmt, clippy" }
      - uses: actions/setup-node@v4
        with: { node-version: "22" }
      - uses: actions/setup-python@v5
        with: { python-version: "3.12" }
      - run: cd runtime && cargo fmt --check
      - run: cd runtime && cargo clippy -- -D warnings
      - run: cd extractors && npm ci && npm run lint
      - run: cd clients/python && pip install -e ".[dev]" && ruff check . && mypy --strict cortex_client/

  test:
    runs-on: ubuntu-latest
    needs: lint
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: browser-actions/setup-chrome@v1
      - uses: actions/setup-node@v4
        with: { node-version: "22" }
      - uses: actions/setup-python@v5
        with: { python-version: "3.12" }
      - run: make build
      - run: make test
```
