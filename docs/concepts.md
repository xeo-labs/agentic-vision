# Concepts

## The Problem

AI agents navigate the web by rendering pages in a browser, taking screenshots, and asking an LLM to find elements. This is slow (20-120 seconds per site), expensive (10-30 LLM calls per navigation), and fragile (pixel coordinates change between renders).

93% of websites serve structured data alongside their HTML — JSON-LD, OpenGraph, Schema.org, platform APIs. This data describes exactly what's on each page: product names, prices, ratings, availability. But current tools ignore it and render anyway.

## The Cortex Approach

Cortex maps websites into **binary graphs** via HTTP, extracting structured data directly. No rendering. No screenshots. No pixel coordinates.

```
Website  ──(HTTP)──→  SiteMap (.ctx binary graph)  ──(query)──→  Structured results
```

Once mapped, the graph is queryable in microseconds. The agent navigates the map, not the site.

## SiteMap Format

A SiteMap is a binary graph file (`.ctx`) containing:

- **Nodes** — One per page. Each node has a URL, page type, 128-dimensional feature vector, confidence score, and list of available actions.
- **Edges** — Links between pages. Each edge has a source, target, and weight.
- **Metadata** — Domain, node count, edge count, creation timestamp, CRC32 checksum.

### Binary Layout

```
+-------------------------------------+
|  HEADER            64 bytes         |  Magic · version · counts · flags
+-------------------------------------+
|  NODE TABLE        ~600 bytes/node  |  url · type · confidence · features[128] · actions
+-------------------------------------+
|  EDGE TABLE        12 bytes/edge    |  source · target · weight
+-------------------------------------+
|  CHECKSUM          4 bytes          |  CRC32
+-------------------------------------+
```

### Size Characteristics

| Nodes | Edges | .ctx Size | JSON Equivalent | Compression |
|:------|:------|:----------|:----------------|:------------|
| 108 | 412 | 66.3 KB | 170.6 KB | 2.6x |
| 1,008 | 4,012 | 611.2 KB | 1.6 MB | 2.6x |
| 10,008 | 40,012 | 5.9 MB | 16.0 MB | 2.7x |

Consistent ~600 bytes per node. A mapped github.com (1,783 nodes, 16,231 edges) compresses to 1.2 MB.

## Page Types

Every node is classified into one of 16 page types:

| Code | Type | Description |
|:-----|:-----|:------------|
| 0x01 | Home | Landing / home page |
| 0x02 | ProductListing | Category or listing page |
| 0x03 | SearchResults | Search results page |
| 0x04 | ProductDetail | Individual product page |
| 0x05 | Cart | Shopping cart |
| 0x06 | Article | Blog post or news article |
| 0x07 | Documentation | Docs / help page |
| 0x08 | Login | Authentication page |
| 0x09 | Checkout | Checkout flow |
| 0x0A | Profile | User profile page |
| 0x0B | ApiEndpoint | API endpoint |
| 0x0C | Media | Video / audio page |
| 0x0D | Form | General form page |
| 0x0E | Dashboard | Dashboard / admin |
| 0x0F | Error | Error page (404, 500) |
| 0x10 | Other | Unclassified |

Classification uses a weighted scoring system based on URL patterns, structured data signals, and content heuristics. Confidence scores range from 0.0 to 1.0.

## Feature Vector (128 dimensions)

Every node has a 128-dimensional feature vector of `f32` values. These dimensions capture structured data about the page.

### Dimension Groups

| Range | Category | Key Dimensions |
|:------|:---------|:---------------|
| 0-15 | **Page identity** | page_type (one-hot), confidence, URL depth, domain authority |
| 16-47 | **Content metrics** | word_count, heading_count, image_count, link_density, form_count, table_count |
| 48-63 | **Commerce** | price (USD), original_price, discount (0-1), availability (0/1), rating (0-5 normalized to 0-1), review_count, shipping, seller_reputation |
| 64-79 | **Navigation** | outbound_links, pagination_depth, breadcrumb_depth, nav_items, search_available, filter_count, sort_options |
| 80-95 | **Trust/safety** | TLS (0/1), domain_age, PII_exposure, tracker_count, authority_score, dark_pattern_indicators |
| 96-111 | **Actions** | action_count, safe_action_ratio, cautious_action_ratio, destructive_action_ratio, auth_required, form_completeness |
| 112-127 | **Session** | login_state, session_duration, page_views, cart_value, A/B_variant |

### Key Dimensions for Queries

| Dimension | Name | Example Query |
|:----------|:-----|:-------------|
| 48 | Price (USD) | `features={48: {"lt": 300}}` |
| 49 | Original price | Sale detection (compare dim 48 vs 49) |
| 50 | Discount | `features={50: {"gt": 0.2}}` (20%+ off) |
| 51 | Availability | `features={51: {"eq": 1.0}}` (in stock) |
| 52 | Rating | `features={52: {"gt": 0.8}}` (4+ stars) |
| 53 | Review count | `features={53: {"gt": 100}}` |
| 80 | TLS (HTTPS) | `features={80: {"eq": 1.0}}` (secure) |
| 96 | Action count | Number of available actions |

### Privacy

Dimensions 112-127 (Session) are **zeroed before sharing** via the Collective Graph. This ensures no user session data leaks when maps are pushed to the registry.

## Acquisition Layers

Cortex uses a 6-layer acquisition engine to map websites via HTTP. Each layer adds data. The browser is a last resort.

### Layer 0 — Discovery
Parse `robots.txt` and `sitemap.xml` to discover all URLs on the domain. Cost: 3-5 HTTP requests. Coverage: ~90% of URLs.

### Layer 1 — Structured Data
HTTP GET each page and extract JSON-LD, OpenGraph tags, Schema.org markup, and meta tags. This is the primary data source. Cost: 1 GET per page. Coverage: 93% of sites have structured data.

### Layer 1.5 — Pattern Engine
Apply CSS selector patterns to extract data from HTML when structured data is sparse. Includes pre-built patterns for Shopify, WooCommerce, Magento, BigCommerce, and generic e-commerce. Cost: 0 (in-memory processing on already-fetched HTML).

### Layer 2 — API Discovery
Probe known platform API endpoints (Shopify `/products.json`, WooCommerce `/wp-json/wc/v3/products`, etc.) for structured product data. Cost: 1-3 HTTP requests per platform. Coverage: 31% of sites.

### Layer 2.5 — Action Discovery
Discover available actions (forms, JS endpoints, platform-specific actions) by analyzing HTML forms and JavaScript sources. Actions like add-to-cart and search are often executable via HTTP POST without a browser.

### Layer 3 — Browser Fallback
For the ~5% of pages with no structured data and heavy client-side rendering, launch Chromium, render the page, and extract data from the DOM. Cost: 2-5 seconds per page. This layer is avoided whenever possible.

### Layer Priority

```
URL Discovery (L0)
    ↓
Structured Data (L1) ← Primary data source (93% of sites)
    ↓
Pattern Engine (L1.5) ← Supplementary extraction
    ↓
API Discovery (L2)   ← Platform-specific enrichment
    ↓
Action Discovery (L2.5)
    ↓
Browser Fallback (L3) ← Last resort (<5% of pages)
```

## Navigation

Once a SiteMap is built, the navigation engine provides four query types:

### Filter
Find nodes matching type and feature criteria. Sub-microsecond at any scale.

```python
products = site.filter(page_type=0x04, features={48: {"lt": 300}}, limit=10)
```

### Pathfind
Dijkstra's shortest path between any two nodes. Returns the node sequence and total weight.

```python
path = site.pathfind(from_node=0, to_node=42)
```

### Similarity
Cosine similarity search over 128-dimensional feature vectors. Finds pages with similar characteristics.

```python
similar = site.similar(node_index=5, limit=10)
```

### Clustering
k-means clustering groups pages by feature similarity. Useful for discovering page categories.

## Web Compiler

The Web Compiler analyzes a binary SiteMap and infers typed schemas:

1. **Schema Inference** — Groups nodes by page type, analyzes feature distributions, infers field names and types
2. **Code Generation** — Generates client libraries in 5 formats:
   - Python (`client.py`)
   - TypeScript (`client.ts`)
   - OpenAPI 3.0 (`openapi.yaml`)
   - GraphQL (`schema.graphql`)
   - MCP (`mcp.json`)

See [Web Compiler Guide](guides/web-compiler.md) for details.

## WQL (Web Query Language)

SQL-like syntax for querying web data:

```sql
SELECT name, price, rating
FROM Product
WHERE price < 200 AND rating > 4.0
ORDER BY price ASC
LIMIT 10
```

WQL is parsed by a recursive-descent parser (6-7 µs) and executed against the in-memory SiteMap. Full pipeline: 9-86 µs depending on graph size.

See [WQL Guide](guides/wql.md) for syntax reference.

## Collective Graph

Maps can be shared via the registry:

- **Push** — Upload a map to the registry (privacy-stripped: session dimensions zeroed)
- **Pull** — Download a previously-mapped domain
- **Delta sync** — Only transmit changes since the last push
- **Garbage collection** — Clean up old map versions

## Temporal Intelligence

Track how pages change over time:

- **History** — Record feature values at each mapping timestamp
- **Trends** — Detect upward/downward trends in any dimension
- **Anomalies** — Flag unusual changes (sudden price drops, stock changes)
- **Predictions** — Project future values based on historical patterns
- **Periodicity** — Detect recurring patterns (weekly sales, seasonal pricing)

See [Temporal Intelligence Guide](guides/temporal.md) for details.
