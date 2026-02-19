# AgenticVision

**Core vision library for AI agents — image capture, CLIP embedding, similarity search, and persistent visual memory.**

[![crates.io](https://img.shields.io/crates/v/agentic-vision.svg)](https://crates.io/crates/agentic-vision)
[![MIT License](https://img.shields.io/badge/license-MIT-blue.svg)](../../LICENSE)

## What it does

AgenticVision gives AI agents persistent visual memory. Capture images, embed them with CLIP ViT-B/32 into 512-dimensional vectors, store them in a compact `.avis` binary format, and query them by similarity, time, or description.

## Install

```bash
cargo install agentic-vision
```

Or add to your `Cargo.toml`:

```toml
[dependencies]
agentic-vision = "0.1"
```

## Usage

```rust
use agentic_vision::{VisionStore, CaptureSource};

let mut store = VisionStore::open("observations.avis")?;

// Capture from file
let id = store.capture(
    CaptureSource::File("screenshot.png"),
    "Homepage after deploy"
)?;

// Find similar
let matches = store.similar(id, 5)?;
for m in matches {
    println!("  {} (similarity: {:.3})", m.description, m.score);
}
```

## Key features

- **CLIP ViT-B/32 embeddings** — 512-dimensional vectors via ONNX Runtime, with fallback mode when model is not present
- **Binary `.avis` format** — 64-byte header, JSON payload, JPEG thumbnails. Single file, portable, no database
- **Similarity search** — Brute-force cosine in 1-2 ms (top-5)
- **Visual diff** — Pixel-level differencing with 8×8 grid region detection in <1 ms
- **Image capture** — From files, base64, screenshots, or clipboard. Auto-resize and JPEG compression. Native screenshot support on macOS (`screencapture`) and Linux (`gnome-screenshot`/`scrot`/`maim`); clipboard capture via `osascript` (macOS) or `xclip`/`wl-paste` (Linux)

## Performance

| Operation | Time |
|:---|---:|
| Image capture (file → embed → store) | **47 ms** |
| Similarity search (top-5) | **1-2 ms** |
| Visual diff (pixel-level) | **<1 ms** |
| Storage per capture | **~4.26 KB** |

## MCP Server

For LLM integration via the Model Context Protocol, see [agentic-vision-mcp](https://crates.io/crates/agentic-vision-mcp).

## Links

- [GitHub](https://github.com/xeo-labs/agentic-vision)
- [MCP Server](https://crates.io/crates/agentic-vision-mcp)
- [AgenticMemory](https://github.com/xeo-labs/agentic-memory) — Persistent cognitive memory for AI agents

## License

MIT
