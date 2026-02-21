# cortex-web-client

Thin TypeScript client for [Cortex](https://github.com/agentralabs/agentic-vision) — the rapid web cartographer for AI agents.

## Install

```bash
npm install cortex-web-client
```

> Requires the Cortex runtime. Install with: `cargo install cortex-runtime`
> The daemon starts automatically on first use.

## Quick Start

```typescript
import { map } from "cortex-web-client";

// Map a website into a navigable graph
const site = await map("amazon.com");
console.log(`Mapped ${site.nodeCount} pages, ${site.edgeCount} links`);

// Find products under $300
const products = await site.filter({
  pageType: 4, // ProductDetail
  features: { 48: { lt: 300 } },
  limit: 10,
});

// Find the shortest path between two pages
const path = await site.pathfind(0, products[0].index);
console.log(`Path: ${path.nodes.join(" → ")}`);
```

## Page Types

| Code | Type | Description |
|------|------|-------------|
| `1` | Home | Landing / home page |
| `2` | ProductListing | Category or listing page |
| `3` | SearchResults | Search results |
| `4` | ProductDetail | Individual product page |
| `5` | Cart | Shopping cart |
| `6` | Article | Blog post or news article |
| `7` | Documentation | Docs / help page |
| `8` | Login | Authentication page |
| `9` | Checkout | Checkout flow |

## Feature Dimensions

Each page has a 128-dimensional feature vector. Key dimensions:

| Dim | Name | Range |
|-----|------|-------|
| 48 | Price (USD) | Absolute |
| 52 | Rating | 0.0 - 5.0 |
| 80 | TLS (HTTPS) | 0 or 1 |
| 96 | Action count | Absolute |

## API

```typescript
import { map, mapMany, perceive, perceiveMany, status } from "cortex-web-client";
import type { SiteMapClient, NodeMatch, MapOptions } from "cortex-web-client";

// Map sites
const site = await map("github.com");
const sites = await mapMany(["amazon.com", "ebay.com"]);

// Query
const results = await site.filter({ pageType: 4 });
const path = await site.pathfind(0, 42);
const similar = await site.similar(5, 10);

// Single page
const page = await perceive("https://example.com/page");

// Runtime status
const info = await status();
console.log(info.cached_maps, info.uptime);
```

## Error Handling

```typescript
import { CortexConnectionError, CortexMapError } from "cortex-web-client";

try {
  const site = await map("example.com");
} catch (err) {
  if (err instanceof CortexConnectionError) {
    console.error("Cortex daemon not running. Start with: cortex start");
  }
}
```

## License

Apache-2.0
