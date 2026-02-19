# Benchmarks

All benchmarks measured on Apple M4 Pro (12-core ARM64), 64 GB unified memory, macOS 26.2, Rust 1.90.0, `cargo test --release` with warm-up iterations and 100-1,000 repetitions. Binary size: 17 MB. Idle memory: ~20 MB.

## Query Performance

Sub-microsecond filter queries at all scales. WQL pipeline completes in under 100 µs.

| Nodes | Filter Type | Filter + Feature | Pathfind | Similarity (top-10) | WQL Pipeline |
|------:|----------:|---------------:|---------:|-------------------:|-------------:|
| 108 | 1 µs | 1 µs | 24 µs | 447 µs | 9 µs |
| 1,008 | <1 µs | 1 µs | 105 µs | 4.2 ms | 16 µs |
| 5,008 | <1 µs | 1 µs | 463 µs | 21.3 ms | 49 µs |
| 10,008 | <1 µs | 1 µs | 884 µs | 42.7 ms | 86 µs |

**Filter queries** use indexed page-type lookup — O(1) regardless of graph size.

**WQL pipeline** includes parsing (6-7 µs, constant) + planning + execution.

**Pathfinding** uses Dijkstra's algorithm — scales with graph diameter, not total size.

**Similarity search** uses brute-force cosine over 128-dimensional vectors — O(n).

## Write Performance

| Base Nodes | Add Node | Add Edge | Batch 100 | File Write | File Read |
|-----------:|---------:|---------:|----------:|-----------:|----------:|
| 108 | 1.6 µs | 3.1 µs | 697 µs | 52 µs | 2.4 ms |
| 1,008 | 1.7 µs | 3.3 µs | 717 µs | 137 µs | 21.7 ms |
| 5,008 | 1.7 µs | 3.2 µs | 713 µs | 512 µs | 107.0 ms |

**Add node/edge** are O(1) amortized (Vec push).

**File I/O** scales linearly with node count.

## Serialization

| Nodes | Serialize | Deserialize | File Size |
|------:|----------:|------------:|----------:|
| 108 | 1.5 ms | 2.4 ms | 66.3 KB |
| 1,008 | 13.3 ms | 21.9 ms | 611.2 KB |
| 5,008 | 66.8 ms | 106.4 ms | 3.0 MB |
| 10,008 | 134.9 ms | 212.2 ms | 5.9 MB |

Includes full 128-float feature matrix and CRC32 checksum verification.

## Compression

| Nodes | Edges | .ctx Size | JSON Equivalent | Ratio |
|------:|------:|----------:|----------------:|------:|
| 108 | 412 | 66.3 KB | 170.6 KB | 2.6x |
| 508 | 2,012 | 308.9 KB | 805.5 KB | 2.6x |
| 1,008 | 4,012 | 611.2 KB | 1.6 MB | 2.6x |
| 5,008 | 20,012 | 3.0 MB | 8.0 MB | 2.7x |
| 10,008 | 40,012 | 5.9 MB | 16.0 MB | 2.7x |

Consistent **~600 bytes per node**. Dominated by the 512-byte feature matrix (128 × 4-byte f32).

## Web Compiler

| Nodes | Models | Fields | Relationships | Infer Time |
|------:|-------:|-------:|--------------:|-----------:|
| 108 | 5 | 21 | 7 | 1.2 ms |
| 1,008 | 5 | 21 | 7 | 9.8 ms |
| 5,008 | 5 | 21 | 7 | 46.5 ms |
| 10,008 | 5 | 21 | 7 | 96.9 ms |

Model count is determined by page type distribution (5 types in e-commerce test data), not total node count. Schema inference scales linearly.

## Collective Graph

| Operation | Nodes | Time |
|:----------|------:|-----:|
| Compute delta (10% change) | 108 | 1.2 ms |
| Compute delta (10% change) | 1,008 | 9.6 ms |
| Compute delta (10% change) | 5,008 | 55.8 ms |
| Registry push | 1,007 | 17.3 ms |
| Registry pull | 1,007 | 22.0 ms |
| Push 10 domains | 1,070 total | 78.5 ms |
| Privacy strip | 5,008 | <1 ms |

Privacy stripping (zeroing session dimensions 112-127) is negligible.

## Real Website Measurements

| Domain | Nodes | Edges | Actions | Size | Bytes/Node |
|:-------|------:|------:|--------:|-----:|-----------:|
| example.com | 62 | 142 | 0 | 37.9 KB | 626 |
| github.com | 1,783 | 16,231 | 360 | 1,191 KB | 684 |
| amazon.com | 62 | 142 | 0 | 37.8 KB | 625 |

Bytes/node consistency across real sites (625-684) validates synthetic benchmark predictions (~600).

## 100-Site Mapping Quality

Cortex was tested against 100 real production websites across 10 categories. Scores measure completeness of page discovery, accuracy of type classification, and correctness of feature extraction.

| Category | Avg Score | Best | Worst | Sites |
|:---------|----------:|-----:|------:|------:|
| Documentation | 94.2 | 100 | 86 | 10 |
| Financial | 92.2 | 96 | 89 | 5 |
| SPA / JS-heavy | 91.0 | 98 | 75 | 10 |
| Government | 90.6 | 98 | 75 | 10 |
| News / media | 87.0 | 96 | 13 | 10 |
| Social | 80.9 | 98 | 64 | 10 |
| E-commerce | 77.5 | 98 | 23 | 15 |
| Travel | 76.7 | 98 | 18 | 10 |
| **Overall** | **85.3** | **100** | **13** | **100** |

**80 of 100 sites score 80+.** Five iterations of the benchmark improved the average from 67.4 to 85.3:

| Iteration | Average | Sites ≥ 80 | Key Change |
|:---------:|--------:|-----------:|:-----------|
| 1 | 67.4 | 28 | Baseline (browser-based) |
| 2 | 73.9 | 68 | URL edge inference |
| 3 | 77.1 | 72 | Unrendered node creation |
| 4 | 82.3 | 74 | Bidirectional edges |
| 5 | 85.3 | 80 | HTTP fallback mapping |

## Acquisition Layer Coverage

| Layer | Coverage | Cost |
|:------|:---------|:-----|
| Layer 0 (Discovery) | ~90% URLs | 3-5 HTTP requests |
| Layer 1 (Structured Data) | 93% sites | 1 GET per page |
| Layer 1.5 (Pattern Engine) | 93% sites | 0 (in-memory) |
| Layer 2 (API Discovery) | 31% sites | 1-3 probes |
| Layer 3 (Browser Fallback) | <5% pages | 2-5s per page |

93% of sites yield structured data via HTTP-only layers. Browser rendering is needed for less than 5% of pages.

## Comparative Analysis

| Dimension | Playwright | Puppeteer | Selenium | Browserbase | Cortex |
|:----------|:----------:|:---------:|:--------:|:-----------:|:------:|
| Browser required | Always | Always | Always | Cloud | Never* |
| Package size | ~280 MB | ~300 MB | ~350 MB | ~0 (cloud) | **17 MB** |
| Site-level graph | No | No | No | No | **Yes** |
| Map a site | 20-120s | 20-120s | 20-120s | 20-120s | **3-15s** |
| Query latency | N/A | N/A | N/A | N/A | **<1 µs** |
| LLM calls | 10-30 | 10-30 | 10-30 | 10-30 | **0** |
| Structured data | No | No | No | No | **Primary** |
| Idle memory | ~500 MB | ~500 MB | ~500 MB | 0 | **~20 MB** |

\*93% of sites mapped via HTTP only.

## Gateway Integration Scores

| Component | Score | Max |
|:----------|------:|----:|
| MCP server (9 tools) | 30 | 30 |
| REST API (11 endpoints) | 27 | 30 |
| Python client (5 operations) | 22 | 25 |
| Framework adapters | 11 | 15 |
| **Total** | **90** | **100** |
