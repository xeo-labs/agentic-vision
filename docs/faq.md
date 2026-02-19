# FAQ

## General

### What is Cortex?
Cortex is a web cartography engine that maps websites into navigable binary graphs. Instead of rendering pages in a browser, it extracts structured data via HTTP and builds a queryable graph of every page, link, and feature.

### Who is Cortex for?
AI agents that need to interact with the web. Instead of launching a browser and navigating page by page, an agent can map an entire site in seconds and query it in microseconds.

### How is this different from a web scraper?
Scrapers extract text from individual pages. Cortex builds a complete site graph — every page typed, every product priced, every link mapped. You can pathfind between pages, filter by features, compare across sites, and track changes over time.

### Do I need a browser?
No. 93% of websites are mapped entirely via HTTP using structured data (JSON-LD, OpenGraph, Schema.org). Chromium is a last-resort fallback for the ~5% of pages with no structured data. Many users never need it.

### What sites work best?
Documentation sites (94% avg), financial sites (92%), and government sites (91%) work best due to rich structured data. E-commerce (78%) and travel (77%) sites have more variation. Sites with heavy bot detection may score lower.

### What sites don't work well?
Sites with aggressive bot detection (e.g., Cloudflare challenges), fully client-rendered SPAs with no structured data, and canvas/WebGL-based applications. See [LIMITATIONS.md](LIMITATIONS.md) for the full list.

---

## Installation

### What are the system requirements?
macOS (arm64/x86_64) or Linux (x86_64/aarch64). Rust 1.75+ for `cargo install`. Python 3.9+ for the Python client. Node.js 18+ for the TypeScript client.

### Does it work on Windows?
Not natively. Use WSL2 or Docker. See [INSTALL.md](../INSTALL.md) for details.

### How big is the binary?
17 MB for the runtime. No external dependencies. Compare: Playwright ~280 MB, Puppeteer ~300 MB, Selenium ~350 MB (all including browser).

### Do I need to install Chromium separately?
No. Cortex downloads Chromium for Testing on-demand only if a page requires browser rendering. Most sites don't need it.

---

## Usage

### How long does mapping take?
3-15 seconds for most sites via HTTP. Sites requiring browser fallback may take longer.

### How much disk space do maps use?
~600 bytes per node. A site with 1,000 pages is about 600 KB. A site with 10,000 pages is about 6 MB. Maps are cached in `~/.cortex/maps/`.

### Can I map password-protected sites?
Yes. Use `cortex_client.login()` or the `cortex_auth` MCP tool to authenticate first, then map with the session.

### What's WQL?
Web Query Language — SQL-like syntax for querying mapped web data. Example: `SELECT name, price FROM Product WHERE price < 200 ORDER BY price ASC LIMIT 10`.

### Can I query across multiple sites?
Yes. Map multiple sites, then use `compare()` or WQL `FROM` clauses to query across all of them.

### What page types are supported?
16 types: Home, ProductListing, SearchResults, ProductDetail, Cart, Article, Documentation, Login, Checkout, Profile, ApiEndpoint, Media, Form, Dashboard, Error, Other.

### What feature dimensions can I filter on?
128 dimensions grouped into Page Identity (0-15), Content Metrics (16-47), Commerce (48-63), Navigation (64-79), Trust/Safety (80-95), Actions (96-111), and Session (112-127). Key dimensions: price (48), rating (52), availability (51), TLS (80).

---

## Agent Integration

### Which AI agents are supported?
Claude Desktop, Claude Code, Cursor, Windsurf, Continue, and Cline. Any MCP-compatible agent works.

### How does `cortex plug` work?
It scans known config file locations for each supported agent, then adds a Cortex MCP server entry to each config. The injection is safe and idempotent — it won't duplicate entries or modify existing configuration.

### Can I undo `cortex plug`?
Yes. Run `cortex plug --remove` to cleanly remove Cortex from all agent configurations.

### What MCP tools does the agent get?
9 tools: `cortex_map`, `cortex_query`, `cortex_pathfind`, `cortex_act`, `cortex_perceive`, `cortex_compare`, `cortex_auth`, `cortex_compile`, `cortex_wql`.

### Which framework adapters are available?
LangChain, CrewAI, AutoGen, Semantic Kernel, and OpenClaw. See the [Integration Guide](integration-guide.md).

---

## Architecture

### What's the binary `.ctx` format?
A compact binary graph with fixed-size node records (~600 bytes/node), 128-dimensional feature vectors, indexed edge tables, and CRC32 integrity checks. 2.7x smaller than equivalent JSON.

### Why not just use JSON?
JSON is 2.7x larger, requires parsing (slow), and has no fixed-size records (no O(1) access). The binary format enables sub-microsecond queries.

### How does the Web Compiler work?
It analyzes the page type distribution in a binary SiteMap, infers field names and types from feature vector patterns, and generates typed client libraries in 5 formats: Python, TypeScript, OpenAPI, GraphQL, MCP.

### What's the Collective Graph?
A registry for sharing maps between agents. Maps are pushed with session dimensions (112-127) zeroed for privacy. Delta sync only transmits changes since the last push.

### How does Temporal Intelligence work?
Each time a site is mapped, feature values are recorded with timestamps. Over time, this builds a history that enables trend detection, anomaly detection, periodicity analysis, and value predictions.

---

## Performance

### How fast are queries?
Filter queries: <1 µs at any scale. WQL: 9-86 µs. Pathfinding: 24-884 µs. Similarity: 0.4-42.7 ms.

### What hardware were benchmarks run on?
Apple M4 Pro, 64 GB unified memory, macOS 26.2, Rust 1.90.0, `--release` with LTO. See [Benchmarks](benchmarks.md) for full data.

### How much memory does Cortex use?
~20 MB idle. Maps are loaded on-demand. Large maps may increase memory proportionally.

---

## Comparison

### How does Cortex compare to Playwright/Puppeteer/Selenium?
Those tools render pages in a browser. Cortex extracts structured data via HTTP. Cortex is 10-40x faster for mapping, requires no browser for 93% of sites, uses 15-20x less memory, and provides a queryable graph instead of raw DOM.

### How does Cortex compare to Browserbase/AgentQL?
Cloud browser services still render every page. Cortex maps sites via HTTP locally. No cloud dependency, no per-page billing, sub-microsecond queries on cached maps.

### Can Cortex replace my browser automation tool?
For data extraction and navigation planning — yes. For actions that require visual interaction (drag-and-drop, canvas, complex forms) — not fully. Cortex handles many actions via HTTP POST but falls back to a browser for complex interactions.
