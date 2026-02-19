# Changelog

All notable changes to AgenticVision will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-02-19

First release. Two crates published to crates.io. 88 tests passing across all suites.

### Added

- **Core Library (`agentic-vision` v0.1.0)**
  - Binary `.avis` file format — 64-byte fixed header (magic `0x41564953`, version, capture count, timestamps), JSON payload with embedded JPEG thumbnails and 512-dim float vectors
  - CLIP ViT-B/32 image embedding via ONNX Runtime (512-dimensional vectors)
  - Deterministic fallback embedding when ONNX model is not present
  - Cosine similarity search with f64 intermediate precision
  - Pixel-level visual diff with 8×8 grid region detection (threshold 30/255)
  - Image capture from file paths, base64 data, and URLs
  - JPEG thumbnail generation and storage
  - Memory-mapped I/O via `memmap2`
  - Published to crates.io: `cargo install agentic-vision`

- **MCP Server (`agentic-vision-mcp` v0.1.0)**
  - 10 MCP tools: `vision_capture`, `vision_compare`, `vision_query`, `vision_ocr`, `vision_similar`, `vision_track`, `vision_diff`, `vision_link`, `session_start`, `session_end`
  - 6 resources via `avis://` URIs (capture, session, timeline, similar, stats, recent)
  - 4 prompt templates: observe, compare, track, describe
  - Stdio transport (MCP protocol v2024-11-05)
  - Session management with named observation sessions
  - Cross-system linking to AgenticMemory `.amem` nodes via `vision_link`
  - Published to crates.io: `cargo install agentic-vision-mcp`

- **Monorepo structure**
  - Cargo workspace with `crates/agentic-vision/` (core) and `crates/agentic-vision-mcp/` (MCP server)
  - Python integration tests in `tests/integration/`
  - Multi-agent scenario tests (shared files, vision-memory linking, rapid handoff)

- **Research Paper**
  - [Paper II: AgenticVision-MCP — 8 pages, 4 TikZ figures, 7 booktabs tables, 18 references](publication/paper-ii-agentic-vision-mcp/agentic-vision-mcp-paper.pdf)
  - All benchmark data from real test runs (zero fabrication)

### Test coverage

- Rust core: 35 tests (unit + integration)
- Python SDK: 47 tests (edge cases, format validation)
- MCP integration: 3 tests (Python → Rust stdio transport)
- Multi-agent: 3 tests (shared file, vision-memory linking, 5-agent rapid handoff)
- Total across all suites: 88 tests

### Performance (Apple M4, macOS 26.2, Rust 1.90.0 --release)

| Operation | Latency |
|:---|---:|
| Image capture (embed + store) | 47 ms |
| Similarity search (top-5) | 1-2 ms |
| Visual diff | <1 ms |
| MCP tool round-trip | 7.2 ms |
| Storage per capture | ~4.26 KB |

[0.1.0]: https://github.com/agentic-revolution/agentic-vision/releases/tag/v0.1.0
