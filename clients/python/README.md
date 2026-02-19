# cortex-client

Thin Python client for [Cortex](https://github.com/cortex-ai/cortex) — the rapid web cartographer for AI agents.

## Install

```bash
pip install cortex-client
```

> Requires the Cortex runtime. Install with: `cargo install cortex-runtime`
> The daemon starts automatically on first use.

## Quick Start

```python
import cortex_client

# Map a website into a navigable graph
site = cortex_client.map("amazon.com")
print(f"Mapped {len(site.nodes)} pages, {len(site.edges)} links")

# Find products under $300
products = site.filter(page_type=0x04, features={48: {"lt": 300}}, limit=10)
for p in products:
    print(f"  {p.url}  price={p.features[48]}")

# Find the shortest path between two pages
path = site.pathfind(0, products[0].index)
print(f"Path: {' → '.join(str(n) for n in path.nodes)}")
```

## Page Types

Use these codes with `page_type=` filters:

| Code | Type | Description |
|------|------|-------------|
| `0x01` | Home | Landing / home page |
| `0x02` | ProductListing | Category or listing page |
| `0x03` | SearchResults | Search results |
| `0x04` | ProductDetail | Individual product page |
| `0x05` | Cart | Shopping cart |
| `0x06` | Article | Blog post or news article |
| `0x07` | Documentation | Docs / help page |
| `0x08` | Login | Authentication page |
| `0x09` | Checkout | Checkout flow |
| `0x0A` | Profile | User profile |
| `0x0B` | ApiEndpoint | API endpoint |
| `0x0C` | Media | Video / audio page |
| `0x0D` | Form | General form page |
| `0x0E` | Dashboard | Dashboard / admin |
| `0x0F` | Error | Error page (404, 500) |

## Feature Dimensions

Each page has a 128-dimensional feature vector. Key dimensions:

| Dim | Name | Example |
|-----|------|---------|
| 48 | Price (USD) | `features={48: {"lt": 300}}` |
| 49 | Original price | Sale detection |
| 50 | Discount | 0.0 - 1.0 |
| 51 | Availability | 1.0 = in stock |
| 52 | Rating | 0.0 - 5.0 |
| 53 | Review count | Absolute count |
| 80 | TLS (HTTPS) | 1.0 = secure |
| 96 | Action count | Available actions |

## API Reference

### Map & Query

```python
# Map a single site
site = cortex_client.map("github.com")

# Map multiple sites
sites = cortex_client.map_many(["amazon.com", "ebay.com"])

# Filter by type + features
products = site.filter(page_type=0x04, features={52: {"gt": 4.0}})

# Pathfind between pages
path = site.pathfind(from_node=0, to_node=42)

# Similarity search
similar = site.similar(node_index=5, limit=10)
```

### Cross-Site Comparison

```python
result = cortex_client.compare(["amazon.com", "ebay.com", "walmart.com"])
print(result.common_types)   # Shared page types
print(result.unique_types)   # Per-site unique types
```

### Single Page Analysis

```python
page = cortex_client.perceive("https://example.com/product/123")
print(page.page_type, page.features, page.actions)
```

### Authentication

```python
session = cortex_client.login("example.com", username="user", password="pass")
site = cortex_client.map("example.com", session=session)
```

### Error Handling

```python
from cortex_client import CortexConnectionError, CortexMapError, CortexTimeoutError

try:
    site = cortex_client.map("example.com")
except CortexConnectionError:
    print("Cortex daemon not running. Start with: cortex start")
except CortexTimeoutError:
    print("Mapping took too long. Try a smaller site or increase timeout.")
except CortexMapError as e:
    print(f"Mapping failed: {e}")
```

## License

Apache-2.0
