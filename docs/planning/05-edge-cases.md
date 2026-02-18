# CORTEX — Edge Case Audit & UI Polish

**Pre-Release Review — February 2026**

> Every surface the user touches, every failure that could happen, every confusing moment that could drive someone away. Fix these before anyone sees the project.

---

## 1. CLI Polish

The CLI is the first thing anyone interacts with. It must be flawless.

### 1.1 `cortex` (no args)

**Current:** Probably prints clap help text.

**Should:** Print a branded, concise summary — not a wall of text.

```
Cortex v0.4.2 — Rapid web cartographer for AI agents

Usage: cortex <command>

Commands:
  map        Map a website into a navigable binary graph
  query      Search a mapped site by type, features, or similarity
  pathfind   Find shortest path between pages on a mapped site
  perceive   Perceive a single live page
  start      Start the Cortex background process
  stop       Stop the Cortex background process
  restart    Restart the background process
  status     Show runtime status and cached maps
  doctor     Check environment and diagnose issues
  install    Download and install Chromium
  plug       Auto-discover AI agents and inject Cortex as an MCP tool
  cache      Manage cached maps (clear, list)
  completions  Generate shell completions (bash/zsh/fish)

Run 'cortex <command> --help' for details on each command.
```

**Edge case:** User types `cortex` in a directory where a `cortex` file/folder already exists. Shouldn't conflict.

### 1.2 `cortex doctor` — Must Be Bulletproof

This is the single most important command. When something doesn't work, this is where users go. Every possible problem must be diagnosed.

**Checks it must perform (in order):**

```
1.  Cortex version                              → print version
2.  OS / architecture                           → print os, arch
3.  Available memory                            → print total, available
4.  Chromium installed?                         → check all known paths
5.  Chromium version                            → run chromium --version
6.  Chromium launches headless?                 → actually try launching it
7.  Shared libraries present? (Linux only)      → check for libnss3, libatk, etc.
8.  Socket path writable?                       → check /tmp/cortex.sock permissions
9.  Is another Cortex process running?          → check PID file + socket connectivity
10. Port conflicts?                             → check if socket already in use by non-Cortex
11. Disk space for maps?                        → check ~/.cortex/ filesystem has >100MB free
12. Node.js available? (for extractor builds)   → check node --version
13. Python available? (for client)              → check python3 --version
14. Maps cache status                           → list cached maps with sizes and freshness
```

**Output format — use colors and symbols:**

```
$ cortex doctor

  Cortex v0.1.0

  System
    ✓ OS:             macOS 15.3 (arm64)
    ✓ Memory:         16.0 GB available
    ✓ Disk:           47.2 GB free at ~/.cortex/

  Browser
    ✓ Chromium:       128.0.6613.84 at ~/.cortex/chromium/chrome
    ✓ Headless test:  launched and closed in 340ms
    
  Runtime
    ✓ Socket path:    /tmp/cortex.sock (writable)
    ✗ Process:        not running
    
  Cache
    ○ Maps cached:    3 (amazon.com, bestbuy.com, example.com)
    ○ Cache size:     47.2 MB
    
  Optional
    ✓ Node.js:        v22.12.0 (for extractor development)
    ✓ Python:         3.12.8 (for cortex-client)

  Status: READY (start with 'cortex start')
```

**When something is wrong — be SPECIFIC about the fix:**

```
  Browser
    ✗ Chromium:       NOT FOUND
                      Fix: run 'cortex install'
                      Or set CORTEX_CHROMIUM_PATH=/path/to/chrome
                      
    ✗ Headless test:  FAILED — missing shared libraries
                      Missing: libnss3, libatk1.0-0, libatk-bridge2.0-0
                      Fix (Ubuntu/Debian): sudo apt install libnss3 libatk1.0-0 libatk-bridge2.0-0
                      Fix (Alpine):        apk add nss atk at-spi2-atk
                      
    ✗ Headless test:  FAILED — no display server
                      This is normal in Docker/CI. Cortex uses headless mode.
                      If you're seeing this on a desktop, check X11/Wayland.

  Runtime
    ⚠ Process:        running (PID 44821) but not responding on socket
                      This usually means a crashed process.
                      Fix: run 'cortex stop' then 'cortex start'
```

**Edge cases for doctor:**
- PID file exists but process is dead (stale PID) → detect and clean up
- Socket file exists but no process is listening → detect, offer to clean up
- Two Cortex processes running (race condition) → detect, show both PIDs
- Chromium binary exists but is wrong architecture (x64 on arm64 Mac) → detect
- User ran `cortex install` but download was interrupted → detect partial install
- Disk is full → detect before mapping fails
- Running inside Docker with --no-new-privileges → Chromium sandbox may fail, detect and suggest `--no-sandbox`

### 1.3 `cortex start` / `cortex stop`

**Edge cases:**
- `cortex start` when already running → "Cortex is already running (PID 44821). Use 'cortex restart' or 'cortex stop' first."
- `cortex stop` when not running → "Cortex is not running." (exit 0, not error)
- `cortex stop` when PID file is stale → clean up PID file, say "Cleaned up stale PID file."
- Process crashes during operation → socket file and PID file left behind. Next `start` must detect and clean up.
- User ctrl+C during start → clean shutdown, remove socket + PID
- `cortex start --foreground` → run in foreground (useful for Docker, debugging)
- `cortex start --port 7700` → listen on TCP instead of socket (for remote access / Docker)
- Multiple users on same machine → use user-specific socket path: `/tmp/cortex-{uid}.sock`

**Add:** `cortex restart` command (stop + start in one step).

### 1.4 `cortex map <domain>`

**Edge cases:**
- Domain doesn't exist → "Error: DNS resolution failed for 'notasite.xyz'. Check the domain name."
- Domain exists but returns 403/401 for everything → "Warning: site returned 403 Forbidden. Mapped 0 renderable pages. The site may require authentication or block automated access."
- Domain redirects to another domain (www.example.com → example.com) → follow redirect, map the final domain, note the redirect in output
- Domain has no sitemap.xml → fall back to crawl, say "No sitemap found. Crawling from entry points..."
- Domain has a massive sitemap (1M+ URLs) → respect max_nodes, say "Sitemap contains 1,247,832 URLs. Mapping first 50,000 (use --max-nodes to change)."
- Domain is localhost/IP address → allow it (for testing internal apps)
- Domain with non-standard port (example.com:8080) → support it
- Domain with path (example.com/blog) → map only under that path
- HTTPS certificate invalid → warn but continue: "Warning: TLS certificate for 'example.com' is invalid. Proceeding anyway."
- Site is extremely slow (>10s per page) → adjust timeouts, warn user
- Site rate-limits aggressively → detect 429 responses, back off, inform user: "Rate limited by site. Slowing down. This may take longer."
- Mapping interrupted (ctrl+C) → save partial map, say "Mapping interrupted. Partial map saved with 1,247 of ~50,000 nodes."
- User runs `cortex map amazon.com` twice → second time uses cache if fresh, say "Using cached map (2 minutes old). Use --fresh to re-map."

**Output polish:**

```
$ cortex map amazon.com

  Mapping amazon.com...
  
  [1/4] Fetching sitemap.xml          47,832 URLs found
  [2/4] Classifying URL patterns       12 page types identified
  [3/4] Rendering sample pages         187/200 ████████████████████ 93%
  [4/4] Building site graph            47,832 nodes, 142,891 edges

  Map complete in 3.4s
  
  ┌─────────────────────────────────────────────┐
  │  amazon.com                                 │
  │  Nodes:     47,832 (187 rendered, rest est.)│
  │  Edges:     142,891                         │
  │  Clusters:  23                              │
  │  Map size:  28.7 MB                         │
  │  Saved to:  ~/.cortex/maps/amazon.com.ctx   │
  │                                             │
  │  Top page types:                            │
  │    product_detail   31,247  (65%)           │
  │    product_listing   4,821  (10%)           │
  │    article           3,102  (6%)            │
  │    review_list       2,847  (6%)            │
  │    other             5,815  (12%)           │
  └─────────────────────────────────────────────┘
  
  Query with: cortex query amazon.com --type product
```

### 1.5 `cortex query <domain>`

**Edge cases:**
- Domain not mapped yet → "Error: no map found for 'bestbuy.com'. Run 'cortex map bestbuy.com' first."
- Query returns 0 results → "No matching pages found. Try broader filters."
- Query returns thousands → show count + first N: "Found 4,821 matching pages. Showing first 20 (use --limit to change)."
- Feature index out of range → "Error: feature dimension 200 doesn't exist. Valid range: 0-127."
- Invalid page type → "Error: unknown page type 'product'. Did you mean 'product_detail' (0x04)?"

**Support both numeric and named filters:**

```bash
# Numeric (for scripts/agents)
cortex query amazon.com --type 0x04 --feature "48<300" --feature "52>0.8"

# Named (for humans)
cortex query amazon.com --type product_detail --price-lt 300 --rating-gt 4.0

# Both should work and produce identical results
```

### 1.6 `cortex pathfind <domain>`

**Edge cases:**
- No path exists between nodes → "No path found from node 0 to node 4821 with current constraints. Try relaxing --avoid flags."
- Path requires authentication → "Shortest path requires login at step 2 (node 341). Use --allow-auth or provide credentials."
- Path is very long (>20 hops) → warn: "Path found (47 hops). This seems unusually long. The agent may want to search for a more direct route."
- Source or target node doesn't exist → "Error: node 99999 doesn't exist in this map (max: 47831)."

### 1.7 `cortex install`

**Edge cases:**
- No internet → "Error: cannot reach Chrome for Testing download server. Check your internet connection."
- Disk full → "Error: not enough disk space. Chromium requires ~300MB. Available: 42MB."
- Download interrupted → resume or clean up partial download and retry
- Wrong permissions → "Error: cannot write to ~/.cortex/. Check permissions."
- Already installed → "Chromium 128.0.6613.84 is already installed. Use --force to reinstall."
- Running inside Docker as root → chromium sandbox issues. Detect and suggest `CORTEX_CHROMIUM_NO_SANDBOX=1`

**Output:**

```
$ cortex install

  Installing Chromium for Cortex...
  
  Platform:  macOS arm64
  Version:   128.0.6613.84 (Chrome for Testing)
  
  Downloading   [████████████████░░░░] 247 MB / 312 MB  79%
  
  ...
  
  ✓ Chromium installed to ~/.cortex/chromium/
  ✓ Verified: launches headless successfully
  
  Run 'cortex doctor' to verify full setup.
```

### 1.8 `cortex status`

**Edge cases:**
- Process not running → "Cortex is not running. Start with 'cortex start'."
- Process running but no maps → show status with empty cache
- Large cache → show top 10 maps by recency, total size

```
$ cortex status

  Cortex v0.1.0 — running (PID 44821, uptime 2h 14m)
  
  Browser Pool
    Active contexts:  2 / 8
    Memory usage:     340 MB / 1000 MB limit
    
  Cached Maps (3)
    amazon.com         28.7 MB   2 min ago    47,832 nodes
    bestbuy.com        12.1 MB   15 min ago   18,421 nodes
    example.com         0.1 MB   1 hour ago      23 nodes
    Total:             40.9 MB
    
  Session: none active
  Audit log: ~/.cortex/audit.jsonl (1,247 entries)
```

---

## 2. Client Library Polish

### 2.1 Python Client DX

**Import experience:**

```python
# This must work on first try. No surprises.
from cortex_client import map

site = map("amazon.com")
# If Cortex isn't running, auto-start should handle it transparently
# If Chromium isn't installed, error should say EXACTLY what to do
```

**Edge cases:**
- Cortex binary not found → "CortexConnectionError: Could not find 'cortex' binary. Install Cortex: https://cortex.dev/install"
- Cortex starts but Chromium not installed → "CortexSetupError: Chromium not installed. Run 'cortex install' in your terminal."
- Socket permission denied → "CortexConnectionError: Permission denied on /tmp/cortex.sock. Check file permissions or run 'cortex stop && cortex start'."
- Map takes longer than expected → should show progress? Or just block? Decision: block with configurable timeout. Default 30s. After timeout: "CortexTimeoutError: Mapping 'amazon.com' exceeded 30s timeout. Use timeout_ms parameter to increase."
- User passes URL instead of domain → `map("https://amazon.com/dp/B0EXAMPLE")` should auto-extract domain "amazon.com" and map the whole site. Or should it map only that path? Decision: extract domain, map whole site, log a note: "Note: mapping entire domain 'amazon.com', not just the provided URL path."
- Empty domain → "ValueError: domain cannot be empty"
- Domain with protocol → strip it: `map("https://example.com")` → maps "example.com"
- Domain with trailing slash → strip it
- Domain with port → preserve it: `map("localhost:3000")` → maps "localhost:3000"

**SiteMap method edge cases:**

```python
site = map("example.com")

# filter with no results
results = site.filter(page_type=0x04, features={48: {"lt": 0}})
# → returns empty list, not None, not error

# pathfind to self
path = site.pathfind(from_node=0, to_node=0)
# → returns path with 0 hops and just the source node

# pathfind with invalid node
path = site.pathfind(from_node=0, to_node=999999)
# → raises CortexPathError("Node 999999 not found in map. Map has 47832 nodes.")

# nearest with zero-vector goal
results = site.nearest([0.0] * 128, k=10)
# → should work, returns nodes closest to zero vector

# nearest with wrong dimension count
results = site.nearest([0.0] * 64, k=10)
# → raises ValueError("Goal vector must be 128 dimensions, got 64")

# refresh already-fresh nodes
result = site.refresh(nodes=[0, 1, 2])
# → should work, re-renders them, returns change report even if nothing changed

# act on a non-existent action
result = site.act(node=0, opcode=(0x02, 0x00))
# → CortexActError("No 'add_to_cart' action found on this page")

# watch with no changes
for delta in site.watch(nodes=[0], interval_ms=1000):
    # If nothing changes for a long time, should this timeout?
    # Decision: stream stays open until explicitly closed or session timeout
    pass
```

**Repr/str for all objects:**

```python
>>> site = map("amazon.com")
>>> site
SiteMap(domain='amazon.com', nodes=47832, edges=142891, cached=True)

>>> results = site.filter(page_type=0x04, limit=3)
>>> results[0]
NodeMatch(index=4821, url='https://amazon.com/dp/B0...', type=product_detail, price=278.0, rating=0.92)

>>> path = site.pathfind(0, 4821)
>>> path
Path(hops=3, nodes=[0, 12, 341, 4821], weight=3.0)
```

### 2.2 TypeScript Client DX

Same edge cases as Python, adapted for TypeScript idioms:
- All methods return Promises
- Errors are typed: `CortexConnectionError`, `CortexTimeoutError`, etc.
- SiteMap methods return typed arrays, not any
- Support both callback and async/await patterns

### 2.3 Error Messages — The Golden Rule

Every error message must answer three questions:
1. **What happened?** (the error)
2. **Why?** (the cause)
3. **How to fix it?** (the action)

```
BAD:  "Error: E_MAP_BLOCKED"
GOOD: "Mapping failed: amazon.com blocked automated access (HTTP 403). 
       Try: (1) wait and retry, (2) use --stealth flag, (3) check if the site requires login."

BAD:  "Connection refused"
GOOD: "Cannot connect to Cortex process at /tmp/cortex.sock.
       The process may not be running. Start it with: cortex start"

BAD:  "E_RENDER_CRASH"  
GOOD: "Browser crashed while rendering https://example.com/heavy-page.
       This page may be too resource-intensive. Try: --timeout 60000 or --memory-limit 2000"
```

---

## 3. Mapping Edge Cases

### 3.1 Site Structure Anomalies

| Edge Case | What Happens | What Should Happen |
|-----------|-------------|-------------------|
| Site is a single-page app (SPA) | Only one URL in sitemap, all content loaded via JS | Detect SPA pattern, render root and extract client-side routes from JS bundle, create virtual nodes for each route |
| Infinite scroll page | Page has no pagination, content loads on scroll | Render initial viewport, detect scroll-loader pattern, create single node with high content density, note "infinite_scroll" flag |
| Site behind Cloudflare/Akamai challenge | JS challenge page before real content | Detect challenge, wait for it to complete (5-10s), then extract. If CAPTCHA, mark node as blocked |
| Site requires JavaScript disabled | Some sites show better content without JS | Try with JS first, if extraction is poor, retry without JS, use whichever gives better results |
| Sitemap lists URLs that 404 | Stale sitemap with dead links | Mark 404 nodes with http_status=404, exclude from navigation edges, include in map for completeness |
| Sitemap is enormous (10M+ URLs) | Memory issue loading sitemap | Stream-parse the sitemap XML, don't load all URLs into memory. Sample from stream. |
| Site has circular redirects | A→B→C→A | Detect redirect loop, mark node as error, break cycle in graph |
| Site uses hash routing (#/page) | URLs differ only in fragment | Treat each unique hash route as a separate node (SPA pattern) |
| Site requires POST to navigate | Form submissions, not links | Map form-submit edges with `requires_form` flag. Agent must use ACT to traverse these edges. |
| Site has inconsistent encoding | Mix of UTF-8, Latin-1, Shift-JIS | Detect encoding per page, normalize all text to UTF-8 before feature extraction |
| Dark web / onion site | .onion domains | Not supported. Clear error: "Tor/onion routing not supported." |
| Very slow site (>10s per page) | Mapping takes forever | Dynamic timeout: start with 5s, if pages are slow, increase to 15s. Report: "Site is slow. Mapping with extended timeouts." |
| Site with login wall | Most content behind auth | Map public pages. Auth-required pages get `auth_required` flag. If credentials provided, create authenticated session for sampling. |

### 3.2 Content Extraction Edge Cases

| Edge Case | What Should Happen |
|-----------|-------------------|
| Price in non-USD currency | Detect currency symbol/code, store in feature vector raw. Don't convert. Agent handles currency comparison. |
| Price range ("$200-$350") | Store low end in features[48], high end as separate feature or use midpoint. Flag as range. |
| Price "From $X" / "Starting at $X" | Store the starting price. Lower confidence on price feature. |
| No price visible (hidden until interaction) | features[48] = 0.0, has_price flag = false. Agent knows to ACT to reveal price. |
| Star rating as images (not text) | Detect star-rating pattern by class names / aria-label. Many sites use CSS sprites for stars. Extract from `aria-label="4.5 out of 5 stars"`. |
| "Add to Cart" button that's actually a link | Classify by behavior, not element type. If `<a>` styled as button with cart-related text, classify as OpCode(0x02, 0x00). |
| Button text in non-English language | Maintain a lookup table of common e-commerce terms in major languages. "Ajouter au panier" → add_to_cart. "カートに追加" → add_to_cart. |
| Cookie consent overlay blocking content | Detect cookie banner (common class names, z-index, position:fixed). Auto-dismiss by finding accept button. If can't dismiss, extract content underneath. |
| Lazy-loaded images below the fold | Scroll the page before extraction, or detect `loading="lazy"` and data-src attributes. Extract the real image URL from data-src. |
| Shadow DOM components | Pierce shadow DOM during extraction. `dom-walker.ts` must handle shadowRoot traversal. |
| Canvas/WebGL rendered content | v0.3.0 added `canvas_extractor.rs`: extract state via accessibility trees (`Accessibility.getFullAXTree()`), internal APIs (e.g., Google Sheets data API, Figma REST API), and app state from JS memory (Redux, globals). Pure pixel-rendered canvases without accessibility data fall back to `has_media` flagging. |
| PDF/document viewer embedded | Detect PDF embed. Node type = download_page. Extract link to PDF, not PDF content. |
| Anti-scraping measures (style-based text, CSS content) | Some sites render text via CSS `content:` property or use custom fonts to scramble characters. Detect low text-to-element ratio as a signal. Set low confidence. |

### 3.3 URL Classification Edge Cases

```
# Ambiguous URLs that classifiers might get wrong

/p/12345              → product? post? page?
  Strategy: check siblings. If most /p/ pages are products, classify as product.

/2026/02/17/          → article (date pattern)
/category/electronics → product_listing
/collections/sale     → product_listing (Shopify pattern)
/t/topic-name         → forum (Discourse pattern)
/wiki/Page_Name       → documentation (MediaWiki pattern)
/issues/123           → could be bug tracker, not a product issue
/r/subreddit          → social_feed (Reddit pattern)
/watch?v=xxxxx        → media_page (YouTube pattern)
/playlist?list=       → product_listing (YouTube, but classified as listing)

# Non-obvious patterns per platform
Shopify:      /products/*, /collections/*, /pages/*
WordPress:    /?p=123, /yyyy/mm/dd/slug, /category/slug
Wix:          /post/*, /product-page/*
Squarespace:  /blog/*, /store/*
```

**Solution:** Ship with a `known-platforms.json` file mapping domain patterns to platform types, and platform types to URL classifiers. Community can contribute new platform patterns.

---

## 4. Protocol Edge Cases

### 4.1 Connection Handling

| Scenario | What Should Happen |
|----------|-------------------|
| Client connects, sends nothing, waits | Server should timeout after 30s of inactivity, close connection |
| Client sends malformed JSON | Return error: `{"error":{"code":"E_INVALID_JSON","message":"..."}}` and keep connection open |
| Client sends valid JSON but unknown method | Return `E_INVALID_METHOD` with list of valid methods |
| Client disconnects mid-request | Server cancels in-progress work (stop rendering), clean up resources |
| Client sends request while previous is still processing | Queue the request, process sequentially per connection. Don't drop it. |
| Two clients send MAP for same domain simultaneously | Deduplicate: only map once, return same result to both. Use a mapping lock per domain. |
| Very large response (100K+ node map as JSON) | Stream the response? Or send file path? Current design: save to file, return path. Correct. |
| Client sends handshake with incompatible version | Return `{"compatible": false, "min_version": "0.1.0"}` and close connection |
| Socket file deleted while process running | Process should detect and recreate socket file. Or clean shutdown. |

### 4.2 Map Request Edge Cases

```json
// Domain that's an IP address
{"method": "map", "params": {"domain": "192.168.1.100"}}
// → Should work for internal/development sites

// Domain with subdomain
{"method": "map", "params": {"domain": "docs.github.com"}}
// → Map only docs.github.com, not all of github.com

// Domain that redirects to a completely different domain
{"method": "map", "params": {"domain": "t.co"}}
// → Follow redirects. Map the final domain. Return note about redirect.

// max_nodes = 0
{"method": "map", "params": {"domain": "example.com", "max_nodes": 0}}
// → Error: "max_nodes must be at least 1"

// max_render > max_nodes
{"method": "map", "params": {"domain": "example.com", "max_nodes": 100, "max_render": 500}}
// → Clamp max_render to max_nodes. Or warn.

// timeout of 1ms (impossibly short)
{"method": "map", "params": {"domain": "example.com", "max_time_ms": 1}}
// → Return partial map (probably just the sitemap parse), with a note about timeout
```

---

## 5. Feature Vector Edge Cases

### 5.1 Normalization Issues

```
Dimension 48 (price_raw): stored as actual USD price, NOT normalized
  - What if price is $0? → Valid (free product). Store 0.0.
  - What if price is $99,999? → Store as-is. Agent compares raw values.
  - What if no price? → Store 0.0 AND set has_price flag to false.
  - What about non-USD? → Store in original currency. Feature does NOT convert.
    Agent must handle currency conversion externally.

Dimension 52 (rating_normalized): rating / max_rating
  - What if rating system is 1-10 instead of 1-5? → Detect max (usually visible in "X out of Y")
  - What if rating is percentage (87%)? → Convert: 87/100 = 0.87
  - What if no rating? → Store 0.0. Agent should check has_price flag analogy (we need a has_rating signal)
  - What if rating is text ("Excellent")? → Map to numeric: Excellent=1.0, Good=0.8, Average=0.6, Poor=0.4, Terrible=0.2

Topic embedding (dims 31-46): 16-dim TF-IDF
  - What if page has no text? → Zero vector
  - What if page is entirely images? → Zero vector + high image_count
  - TF-IDF vocabulary: must be pre-computed and shipped with Cortex. Fixed vocabulary.
  - Out-of-vocabulary words: ignored (standard TF-IDF behavior)
```

### 5.2 Missing Data Strategy

Every feature dimension needs a defined behavior when data is unavailable:

```
RULE: 0.0 means "absent/unknown" for ALL optional features.
      Flags (has_price, has_media, etc.) distinguish "zero" from "unknown".
      
Example:
  features[48] = 0.0, flags.has_price = true   → price is genuinely $0 (free)
  features[48] = 0.0, flags.has_price = false   → price not found on page
  
  features[52] = 0.0, node is rendered          → no rating found
  features[52] = 0.0, node is estimated         → rating unknown (not rendered)
  
  features[52] = 0.72, confidence = 0.95        → high confidence rating
  features[52] = 0.72, confidence = 0.50        → estimated/interpolated rating
```

---

## 6. Installation & Setup Edge Cases

### 6.1 Platform-Specific Issues

**macOS:**
- Apple Silicon (arm64) vs Intel (x64) → install script must detect and download correct binary
- macOS Gatekeeper blocks unsigned Chromium binary → detect, show fix: `xattr -cr ~/.cortex/chromium/`
- macOS sandboxing (App Sandbox) if distributed via .app → unlikely for CLI tool, but document
- Keychain access for credential vault → may trigger permission dialog

**Linux:**
- Missing shared libraries for Chromium → `cortex doctor` must list exact missing libs + install command for Ubuntu/Debian/Fedora/Alpine/Arch
- Wayland vs X11 → headless Chromium doesn't need display server, but detect if user is confused
- SELinux / AppArmor may block Chromium → detect and advise
- Running as root in Docker → Chromium needs `--no-sandbox` flag. Auto-detect Docker and add flag.
- musl libc (Alpine) → Chromium doesn't run on musl natively. Need gcompat or specific build. Detect and advise.

**Windows (future, but plan for it):**
- Named pipe instead of Unix socket → protocol layer must abstract socket path
- Path separators → use PathBuf everywhere, never hardcode `/`
- Process management → different from POSIX signals
- Long path issue (>260 chars) → ~/.cortex/ path should be short

### 6.2 First-Run Experience

The absolute first time someone uses Cortex:

```bash
$ cortex map example.com

  Cortex is not set up yet. Let's get you started:
  
  [1/2] Installing Chromium...
        Downloading Chrome for Testing (247 MB)
        [████████████████████] 100% — 12.3s
        ✓ Installed to ~/.cortex/chromium/
        
  [2/2] Starting Cortex process...
        ✓ Running (PID 44821)
  
  Now mapping example.com...
  ...
```

**The user should NEVER have to run `cortex install` and `cortex start` separately on first use.** `cortex map` (or any command that needs the runtime) should auto-install and auto-start if needed.

### 6.3 Upgrade Path

```bash
$ cortex update
  
  Current:  v0.1.0
  Latest:   v0.2.0
  
  Changes:
    - Improved SPA detection
    - New extractors: recipe sites, news sites
    - Bug fix: shadow DOM traversal crash
    - Performance: 40% faster sitemap parsing
    
  Update? [Y/n] y
  
  Downloading cortex v0.2.0...
  ✓ Installed
  
  Note: Cached maps from v0.1.0 are compatible.
  
  Restart? [Y/n] y
  Restarting... ✓ Running v0.2.0
```

**Edge cases:**
- Upgrade changes binary map format → old maps incompatible. Detect and re-map: "Cached maps from v0.1.0 are incompatible with v0.2.0 format. They will be re-mapped on next access."
- Upgrade while mapping is in progress → don't allow: "Cortex is currently mapping. Stop mapping first."
- Rollback → `cortex update --rollback` restores previous version

---

## 7. Security Edge Cases

### 7.1 Credential Vault

- Master key lost → credentials unrecoverable. Document this clearly.
- Vault file corrupted → detect, offer to reset (losing all credentials)
- Multiple users sharing a machine → vaults are per-user (~/.cortex/ is per-user)
- Credentials stored for a domain that's been compromised → no automatic detection. User responsibility.
- CORTEX_VAULT_KEY env var visible in process list → document: use a file reference instead: `CORTEX_VAULT_KEY_FILE=/path/to/key`

### 7.2 Action Sandbox

- XSS in action values → sandbox must HTML-encode all values before injection into page
- SQL injection in form fills → sandbox blocks `'; DROP TABLE` patterns but this isn't our DB, it's the target site's. Should we block it? Decision: warn but allow. The agent might legitimately need to enter SQL.
- File path traversal in upload actions → block `../` patterns
- Action on a page that's changed since map → stale Manifold. ACT should re-perceive before acting and verify the action still exists. Return `E_ACT_STALE` if page changed.

### 7.3 Privacy

- Audit log contains URLs visited → user must be aware. Document in quickstart.
- Maps contain URL patterns that reveal browsing behavior → maps stored locally, not transmitted
- Cached maps of sensitive sites (banking, health) → provide `cortex cache clear <domain>` and `cortex cache clear --all`
- Agent using Cortex to map a site it shouldn't → not Cortex's problem, but document ethical use guidelines

---

## 8. Performance Edge Cases

### 8.1 Memory

| Scenario | Risk | Mitigation |
|----------|------|------------|
| Mapping a 100K+ page site | 60MB+ map in memory | Configurable max_nodes. Default 50K. Warning at >75% of limit. |
| 10 maps loaded simultaneously | 300MB+ | LRU eviction of least-recently-used maps from in-memory cache. Reload from disk when needed. |
| Browser pool leaks contexts | Memory creep over time | Periodic health check: kill contexts idle >5 min. Max context age: 30 min. |
| Large feature matrix operations | CPU spike during nearest-neighbor on 50K nodes | Use SIMD where possible. KD-tree index for maps >10K nodes. Warn if query takes >1s. |
| Extraction scripts leak in browser | Memory grows per page rendered | Create fresh browser context per N pages (default: 50). Dispose and recreate. |

### 8.2 Concurrency

| Scenario | Risk | Mitigation |
|----------|------|------------|
| Two agents call MAP for same domain | Duplicate work, resource waste | Mapping lock per domain. Second caller waits for first to finish, gets same result. |
| Agent calls QUERY while MAP is in progress | Map not ready yet | Return current partial map with warning, or block until map complete (configurable). |
| REFRESH and QUERY hitting same map concurrently | Read-write conflict | RwLock on map: QUERY takes read lock, REFRESH takes write lock. Queries don't block each other. |
| 100 concurrent PERCEIVE requests | Browser pool exhaustion | Queue with configurable max queue depth. Return E_POOL_EXHAUSTED if queue full. Show queue position. |

---

## 9. Documentation Edge Cases

### 9.1 Quickstart Must Actually Work

Test the quickstart on a CLEAN machine (or fresh Docker container) before publishing:

```bash
# This must work on a fresh Ubuntu 22.04 with only curl installed
curl -fsSL https://cortex.dev/install | sh
cortex map example.com
# Must succeed within 60 seconds of starting
```

Common quickstart failures:
- Missing system dependencies not mentioned in quickstart
- Chromium download URL changed
- Example domain (example.com) is unreachable (unlikely but possible)
- User's network blocks Chromium download (corporate firewall)

### 9.2 Error Documentation

Publish a page listing every error code with:
- Code: E_MAP_BLOCKED
- Description: The target site blocked automated access
- Common causes: Bot detection, IP ban, rate limiting, Cloudflare challenge
- Solutions: 1. Wait and retry 2. Use --stealth 3. Use a different IP 4. Check if site allows bots in robots.txt
- Related: E_RENDER_BLOCKED, E_CAPTCHA

---

## 10. Polish Checklist

### CLI
- [ ] Colored output (disable with `--no-color` or `NO_COLOR=1` env)
- [ ] Progress bars for long operations (mapping, rendering)
- [ ] `--json` flag on every command for machine-readable output
- [ ] `--quiet` flag to suppress non-essential output
- [ ] `--verbose` / `-v` flag for debug logging
- [ ] Tab completion script for bash/zsh/fish
- [ ] Man page generation from clap
- [ ] Consistent exit codes: 0=success, 1=error, 2=usage error
- [ ] All output to stderr except the actual data (so pipes work)

### Client Libraries
- [ ] All errors have `.code` attribute for programmatic handling
- [ ] All objects have meaningful `__repr__` / `toString`
- [ ] Async versions of all methods (Python: `async def amap(...)`)
- [ ] Context managers for sessions (`with Session() as s:`)
- [ ] Type stubs / inline types for IDE autocomplete
- [ ] Docstrings on every public method with examples

### Protocol
- [ ] Request ID uniqueness enforced (reject duplicate IDs)
- [ ] Graceful handling of oversized messages (>10MB)
- [ ] Keepalive/heartbeat on long-lived connections (WATCH)
- [ ] Connection timeout configurable per-client

### Maps
- [ ] Map file integrity check (checksum in header)
- [ ] Map file locking (prevent concurrent writes)
- [ ] Map file versioning (header.format_version checked on load)
- [ ] Compressed map storage option (gzip for disk, uncompressed in memory)

### Security
- [ ] Rate limit client connections (max 10 per second per client)
- [ ] Max map size limit (configurable, default 200MB)
- [ ] Credential vault auto-locks after inactivity
- [ ] Audit log rotation (max 100MB, rotate to .1, .2, etc.)

### Testing
- [ ] Fuzz test the protocol parser (random JSON)
- [ ] Fuzz test the sitemap parser (malformed XML)
- [ ] Stress test: map 10 sites simultaneously
- [ ] Memory leak test: map 100 sites sequentially, check RSS stays bounded
- [ ] Platform test: CI runs on Ubuntu, macOS-arm64, macOS-x64

---

## 11. Known Limitations to Document Honestly

Don't hide these. Put them in a LIMITATIONS.md:

1. **SPAs with client-side routing are partially supported.** Cortex renders the initial page and extracts visible routes, but deeply nested routes that require user interaction to reveal may be missed.

2. **CAPTCHAs are not solved.** If a site presents a CAPTCHA, that node is marked as blocked. Cortex does not integrate CAPTCHA-solving services.

3. **Feature vectors are heuristic.** The 128-dimension encoding captures common web page properties but cannot represent every possible page attribute. Specialized domains (medical, legal, scientific) may need custom extractors.

4. **Interpolated features are estimates.** Unrendered pages have feature vectors estimated from similar rendered pages. These estimates may be inaccurate, especially for prices and availability which change per-product.

5. **Mapping speed depends on the site.** Sites with sitemaps map in 1-3 seconds. Sites without sitemaps require crawling and take 5-15 seconds. Very large sites (>100K pages) may take 30+ seconds.

6. **Most actions can be mapped (v0.3.0+).** Drag-and-drop is ~95% solvable via semantic API replay (`drag_discovery.rs`). Canvas app state is extractable via accessibility trees and internal APIs (`canvas_extractor.rs`). WebSocket-based actions use native WS connections (`ws_discovery.rs`). Only ~5% of truly custom widget interactions remain unsupported.

7. **Currency is not converted.** Price features store raw values in the page's original currency. Cross-site price comparison across currencies is the agent's responsibility.

8. **Rate limiting is best-effort.** Cortex respects robots.txt crawl-delay and self-limits to 5 concurrent requests, but aggressive crawling may still trigger rate limits on sensitive sites.

9. **WebSocket discovery is pattern-based (v0.3.0+).** Discovery depends on recognizable patterns (`new WebSocket(...)`, Socket.IO, SockJS, SignalR) and a curated platform list (`ws_platforms.json`). Sites using custom WebSocket implementations without matching patterns will not have their endpoints discovered. Edge cases: binary WebSocket protocols, authentication-gated WebSocket connections, and connection drops during long-running sessions.

10. **WebMCP adoption is near-zero (v0.4.0+).** `navigator.modelContext` is a new standard. As of early 2026, virtually no production sites have adopted it. The detection mechanism is ready for when adoption increases. Edge cases: sites with partial WebMCP support, tool parameter validation failures, and browser context requirements for WebMCP execution.

11. **`cortex plug` agent detection is heuristic.** Agent detection relies on known config file paths and patterns. Edge cases: agent not found (custom install path), config file permission errors, already-injected detection, and conflicting MCP server entries.

---

*Review this document. Fix everything listed. Then test. Then publish.*
