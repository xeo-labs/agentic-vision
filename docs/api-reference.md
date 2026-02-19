# API Reference

Cortex exposes its functionality through five interfaces: CLI, Python client, TypeScript client, REST API, and MCP tools.

## CLI

### `cortex map <domain>`

Map a website into a binary graph.

```bash
cortex map amazon.com
cortex map amazon.com --max-nodes 10000 --timeout 60
cortex map amazon.com --no-browser      # Skip browser fallback
cortex map amazon.com --json            # Machine-readable output
```

| Flag | Default | Description |
|:-----|:--------|:------------|
| `--max-nodes` | 50000 | Maximum nodes to map |
| `--timeout` | 30 | Timeout in seconds |
| `--no-browser` | false | Skip Chromium fallback |
| `--json` | false | JSON output |
| `--quiet` | false | Suppress progress output |

### `cortex compile <domain>`

Generate typed client libraries from a mapped site.

```bash
cortex compile amazon.com
cortex compile amazon.com --format python
cortex compile amazon.com --output ./my-clients/
```

| Flag | Default | Description |
|:-----|:--------|:------------|
| `--format` | all | Output format: python, typescript, openapi, graphql, mcp, all |
| `--output` | `~/.cortex/compiled/<domain>/` | Output directory |

### `cortex wql "<query>"`

Execute a WQL query against cached maps.

```bash
cortex wql "SELECT name, price FROM Product WHERE price < 200 ORDER BY price ASC LIMIT 10"
cortex wql "SELECT url, page_type FROM Node LIMIT 20" --json
```

### `cortex query <domain>`

Search a mapped site by type and features.

```bash
cortex query amazon.com --type product_detail --price-lt 100 --rating-gt 4.0 --limit 20
cortex query amazon.com --type article --limit 10 --json
```

### `cortex pathfind <domain>`

Find shortest path between nodes.

```bash
cortex pathfind amazon.com --from 0 --to 42
```

### `cortex perceive <url>`

Analyze a single live page.

```bash
cortex perceive "https://amazon.com/dp/B0ABCDEF"
cortex perceive "https://amazon.com/dp/B0ABCDEF" --include-content
```

### `cortex history <domain> <url>`

Query temporal feature history.

```bash
cortex history amazon.com "https://amazon.com/dp/B0ABCDEF" --dim price --since 2026-01-01
```

### `cortex patterns <domain> <url>`

Detect temporal patterns (trends, anomalies, periodicity).

```bash
cortex patterns amazon.com "https://amazon.com/dp/B0ABCDEF" --dim price
```

### `cortex plug`

Auto-discover AI agents and inject Cortex tools via MCP.

```bash
cortex plug              # Auto-discover and inject
cortex plug --status     # Check injection status
cortex plug --remove     # Clean removal
cortex plug --list       # List detected config files
```

### `cortex doctor`

Check environment and diagnose issues.

### `cortex start` / `stop` / `restart` / `status`

Manage the background daemon.

```bash
cortex start                       # Start daemon
cortex start --http-port 7700      # Start with REST API
cortex stop                        # Stop daemon
cortex restart                     # Restart daemon
cortex status                      # Show status + cached maps
```

### Global Flags

| Flag | Description |
|:-----|:------------|
| `--json` | Machine-readable JSON output |
| `--quiet` | Suppress all non-essential output |
| `--verbose` | Enable debug logging |
| `--no-color` | Disable colored output |

---

## Python Client

```bash
pip install cortex-client
```

### `cortex_client.map(domain, **kwargs)`

Map a website and return a `SiteMap` object.

```python
import cortex_client

site = cortex_client.map("amazon.com")
site = cortex_client.map("amazon.com", max_nodes=10000, timeout_ms=60000)

print(len(site.nodes))   # Number of pages
print(len(site.edges))   # Number of links
```

### `cortex_client.map_many(domains)`

Map multiple sites.

```python
sites = cortex_client.map_many(["amazon.com", "ebay.com"])
```

### `site.filter(page_type=None, features=None, limit=20)`

Search nodes by type and feature ranges.

```python
# Find all product pages
products = site.filter(page_type=0x04)

# Products under $300 with rating > 4.0
products = site.filter(
    page_type=0x04,
    features={48: {"lt": 300}, 52: {"gt": 0.8}},
    limit=10
)
```

Page type codes: `0x01` Home, `0x02` ProductListing, `0x03` SearchResults, `0x04` ProductDetail, `0x05` Cart, `0x06` Article, `0x07` Documentation, `0x08` Login, `0x09` Checkout, `0x0A` Profile, `0x0B` ApiEndpoint, `0x0C` Media, `0x0D` Form, `0x0E` Dashboard, `0x0F` Error.

### `site.pathfind(from_node, to_node)`

Find shortest path between two nodes (Dijkstra).

```python
path = site.pathfind(0, 42)
print(path.nodes)         # [0, 5, 12, 42]
print(path.hops)          # 3
print(path.total_weight)  # 1.5
```

### `site.similar(node_index, limit=10)`

Find similar pages by cosine similarity over 128-dim feature vectors.

```python
similar = site.similar(5, limit=10)
for node in similar:
    print(f"  {node.url} (similarity: {node.score:.3f})")
```

### `cortex_client.perceive(url)`

Analyze a single live page.

```python
page = cortex_client.perceive("https://amazon.com/dp/B0ABCDEF")
print(page.page_type)    # "ProductDetail"
print(page.features[48]) # Price
print(page.actions)      # Available actions
```

### `cortex_client.compare(domains)`

Compare across multiple sites.

```python
result = cortex_client.compare(["amazon.com", "ebay.com", "walmart.com"])
print(result.common_types)  # Page types found on all sites
print(result.unique_types)  # Per-site unique types
```

### `cortex_client.login(domain, username, password)`

Authenticate with a site.

```python
session = cortex_client.login("example.com", username="user", password="pass")
site = cortex_client.map("example.com", session=session)
```

### `cortex_client.wql(query)`

Execute a WQL query.

```python
results = cortex_client.wql("SELECT name, price FROM Product WHERE price < 200 LIMIT 10")
for r in results:
    print(f"  {r['name']}: ${r['price']}")
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

---

## TypeScript Client

```bash
npm install @cortex-ai/client
```

### `map(domain, options?)`

```typescript
import { map, mapMany, perceive, status } from "@cortex-ai/client";
import type { SiteMapClient, NodeMatch, MapOptions } from "@cortex-ai/client";

const site = await map("amazon.com");
const site = await map("amazon.com", { maxNodes: 10000, timeoutMs: 60000 });

console.log(site.nodeCount);
console.log(site.edgeCount);
```

### `mapMany(domains)`

```typescript
const sites = await mapMany(["amazon.com", "ebay.com"]);
```

### `site.filter(options)`

```typescript
const products = await site.filter({
  pageType: 4,
  features: { 48: { lt: 300 }, 52: { gt: 0.8 } },
  limit: 10,
});
```

### `site.pathfind(from, to)`

```typescript
const path = await site.pathfind(0, 42);
console.log(path.nodes);       // [0, 5, 12, 42]
console.log(path.hops);        // 3
console.log(path.totalWeight);  // 1.5
```

### `site.similar(nodeIndex, limit?)`

```typescript
const similar = await site.similar(5, 10);
```

### `perceive(url)`

```typescript
const page = await perceive("https://amazon.com/dp/B0ABCDEF");
console.log(page.pageType, page.features, page.actions);
```

### `status()`

```typescript
const info = await status();
console.log(info.cached_maps, info.uptime);
```

### Error Handling

```typescript
import { CortexConnectionError, CortexMapError } from "@cortex-ai/client";

try {
  const site = await map("example.com");
} catch (err) {
  if (err instanceof CortexConnectionError) {
    console.error("Cortex daemon not running. Start with: cortex start");
  }
}
```

---

## REST API

Start the daemon with `--http-port` to enable the REST API:

```bash
cortex start --http-port 7700
```

### Endpoints

| Method | Path | Description |
|:-------|:-----|:------------|
| POST | `/api/v1/map` | Map a domain |
| POST | `/api/v1/query` | Query a mapped domain |
| POST | `/api/v1/pathfind` | Find shortest path |
| POST | `/api/v1/perceive` | Analyze a single URL |
| POST | `/api/v1/act` | Execute an action |
| POST | `/api/v1/auth` | Authenticate with a domain |
| POST | `/api/v1/compile` | Compile schema |
| POST | `/api/v1/wql` | Execute WQL query |
| GET | `/api/v1/status` | Runtime status |
| GET | `/api/v1/events` | Server-Sent Events stream |
| GET | `/dashboard` | Web dashboard |

### Example: Map a domain

```bash
curl -X POST http://localhost:7700/api/v1/map \
  -H "Content-Type: application/json" \
  -d '{"domain": "example.com", "max_nodes": 1000}'
```

### Example: Query products

```bash
curl -X POST http://localhost:7700/api/v1/query \
  -H "Content-Type: application/json" \
  -d '{"domain": "amazon.com", "page_type": 4, "features": {"48": {"lt": 300}}, "limit": 10}'
```

### Example: WQL

```bash
curl -X POST http://localhost:7700/api/v1/wql \
  -H "Content-Type: application/json" \
  -d '{"query": "SELECT name, price FROM Product WHERE price < 200 LIMIT 10"}'
```

---

## MCP Tools

9 tools available via the MCP server. Auto-injected by `cortex plug` or manually via `npx @cortex/mcp-server`.

| Tool | Description |
|:-----|:------------|
| `cortex_map` | Map a website into a navigable binary graph |
| `cortex_query` | Search mapped site by page type, features, or text |
| `cortex_pathfind` | Find shortest path between two pages |
| `cortex_act` | Execute an action (add-to-cart, search, submit, etc.) |
| `cortex_perceive` | Get live state of a single page |
| `cortex_compare` | Compare across multiple mapped sites |
| `cortex_auth` | Authenticate with a website |
| `cortex_compile` | Compile schema and generate clients |
| `cortex_wql` | Execute a WQL query |

See [Integration Guide](integration-guide.md) for MCP setup details.
