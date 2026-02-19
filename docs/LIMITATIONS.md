# Known Limitations

Cortex v1.0 uses a layered HTTP acquisition architecture with advanced action discovery, a Web Compiler for typed schema inference, a Collective Web Graph for map sharing, Temporal Intelligence for change tracking, and WQL for cross-site queries. Most mapping is done without a browser.

## 1. Sites Without Structured Data or Useful HTML Patterns (~2-5% of sites)

Some sites have no JSON-LD, no OpenGraph tags, and no consistent HTML patterns that the CSS selector database can match. These sites fall through to Layer 3 (browser rendering). If the site also blocks headless Chrome, the resulting map will have low-confidence features.

## 2. Actions That Require a Browser (~10-15% of actions)

Drag-and-drop, canvas-based UIs, complex multi-step wizards, and custom web components cannot be executed via HTTP POST. These require browser-based action execution. The action catalog marks such actions with `browser_required=true`.

## 3. OAuth Authentication Requires a One-Time Browser Session

Password-based login works entirely via HTTP (form discovery + POST). However, OAuth flows (Google, GitHub, etc.) require a brief browser session for the consent screen. After the initial OAuth grant, subsequent requests use the captured session cookies without a browser.

## 4. Canvas/WebGL Applications Cannot Be Mapped via HTTP

Applications like Figma, Google Sheets, and other canvas-based tools render their content entirely in WebGL/Canvas. There is no HTML structure to extract. These applications require full browser rendering for any useful data.

## 5. CAPTCHA Solving Is Not Supported

Cortex does not integrate CAPTCHA-solving services. However, CAPTCHAs are rarely triggered in v0.2 because mapping uses standard HTTP requests (not headless Chrome), which are indistinguishable from search engine crawlers.

## 6. WebSocket Discovery Is Pattern-Based

Cortex v0.3+ includes WebSocket endpoint discovery via known platform registries and JS source scanning. However, discovery depends on recognizable patterns (`new WebSocket(...)`, Socket.IO, SockJS, SignalR) and a curated platform list. Sites using custom WebSocket implementations without matching patterns will not have their endpoints discovered. Discovery does not execute the connection — the agent must connect separately.

## 7. Feature Vectors Are Heuristic

The 128-dimension encoding captures common web page properties (prices, ratings, content density, navigation structure) but cannot represent every possible page attribute. Specialized domains may benefit from custom extractors that populate domain-specific feature dimensions.

## 8. Rate Limiting Is Best-Effort

Cortex respects `robots.txt` crawl-delay directives and self-limits concurrent requests per domain. However, aggressive mapping of sensitive sites may still trigger server-side rate limits. If a site returns 429 responses, Cortex backs off with exponential delay, but persistent rate limiting will result in a partial map.

## 9. Currency Is Not Converted

Price features (dimension 48) store raw numeric values in whatever currency the page displays. Cross-site price comparison across currencies is the agent's responsibility.

## 10. Platform Detection Coverage Is Incomplete

The platform action database (`platform_actions.json`) covers Shopify, WooCommerce, Magento, BigCommerce, Squarespace, Wix, PrestaShop, OpenCart, WordPress, Drupal, and Next.js Commerce. Sites on other platforms or custom builds will not get platform-specific action templates, though generic form-based and JS API action discovery still works.

## 11. WebMCP Adoption Is Near-Zero

WebMCP (`navigator.modelContext`) is a new standard for exposing site capabilities as MCP tools. Cortex v0.4 includes detection and execution support, but as of early 2026, virtually no production sites have adopted WebMCP. The detection mechanism is ready for when adoption increases.

## 12. Live Verification Requires Chromium

The `perceive` command and live page analysis require a working Chromium installation. In HTTP-only mode (when Chromium is unavailable), mapping and querying work fully, but `perceive` returns errors. Run `cortex install` to set up Chromium.

## 13. Timeout-Sensitive Sites

Some sites with complex sitemaps, heavy JavaScript, or slow server responses may exceed the default 30-second mapping timeout. Sites like bestbuy.com, netflix.com, and washingtonpost.com consistently require longer timeouts. Use `max_time_ms` parameter to increase the timeout for known slow sites.

## 14. Temporal Intelligence — Sparse Data Limitations (v1.0)

Temporal pattern detection requires sufficient historical data:

- **Trend detection** requires at least 3 data points. With fewer, the engine returns `None` rather than an unreliable result.
- **Predictions** use simple linear regression. They work well for linear trends but cannot capture seasonal, exponential, or other non-linear patterns.
- **Anomaly detection** needs 5+ data points to establish a meaningful baseline. With less data, anomalies cannot be reliably distinguished from normal variation.
- **Periodicity detection** requires at least 2 full cycles of the pattern to detect. Weekly patterns need 2+ weeks of data.

Cortex is honest about these limitations: it returns `None`/empty rather than fabricating unreliable patterns.

## 15. Schema Inference Is Heuristic (v1.0)

The Web Compiler infers schemas from feature vector distributions, not from source code analysis:

- Sites with very few pages (< 3) may not produce useful models
- Mixed-content pages that don't fit standard page types may be misclassified
- Custom e-commerce platforms with non-standard feature patterns may produce incomplete Product models
- Generated client code is a starting point; complex business logic needs manual additions

## 16. WQL Query Language Limitations (v1.0)

WQL is a subset of SQL designed for web data:

- Domain names with hyphens (`my-site.com`) are not supported in ACROSS clauses — use the programmatic API instead
- Temporal functions (`PREDICTED()`, `TREND()`, `HISTORY()`) are parsed but executor support is in progress
- JOIN queries are parsed but cross-model joins are not yet executed
- No aggregation functions (`COUNT`, `SUM`, `AVG`) yet
- No subqueries or nested expressions

## 17. Collective Graph Is Local-Only (v1.0)

The map registry currently supports only local push/pull operations:

- Peer-to-peer synchronization between Cortex instances is planned for v2.0
- Privacy stripping is conservative — all session features (dims 112-127) are zeroed before sharing
- There is no authentication or access control on the local registry
