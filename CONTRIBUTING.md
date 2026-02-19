# Contributing to AgenticVision

Thank you for your interest in contributing to AgenticVision! This document provides guidelines for contributing to the project.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/YOUR_USERNAME/agentic-vision.git`
3. Create a feature branch: `git checkout -b my-feature`
4. Make your changes
5. Run the tests (see below)
6. Commit and push
7. Open a pull request

## Development Setup

This is a Cargo workspace monorepo. All Rust crates are under `crates/`.

### Rust Workspace

```bash
# Build everything (core + MCP server)
cargo build

# Run all tests (core + MCP)
cargo test --workspace

# Core library only
cargo test -p agentic-vision

# MCP server only
cargo test -p agentic-vision-mcp

# Run the MCP server
cargo run -p agentic-vision-mcp -- serve --vision test.avis
```

### Python Integration Tests

```bash
# Requires release build
cargo build --release

# MCP client tests
python tests/integration/test_mcp_clients.py

# Multi-agent scenario tests
python tests/integration/test_multi_agent.py
```

## Ways to Contribute

### Report Bugs

File an issue with:
- Steps to reproduce
- Expected behavior
- Actual behavior
- System info (OS, Rust version)

### Add an MCP Tool

1. Add the tool handler in `crates/agentic-vision-mcp/src/`
2. Register it in the tools list
3. Add tests
4. Update the README tool table

### Write Examples

1. Add a new example in `examples/`
2. Ensure it runs without errors
3. Add a docstring explaining what it demonstrates

### Improve Documentation

All docs are in `docs/`. Fix typos, add examples, clarify explanations — all welcome.

## Code Guidelines

- **Rust**: Follow standard Rust conventions. Run `cargo clippy` and `cargo fmt`.
- **Tests**: Every feature needs tests. We maintain 88+ tests across the stack (35 Rust core, 6 MCP integration + multi-agent, 47 Python).
- **Documentation**: Update docs when changing public APIs.

## Commit Messages

Use clear, descriptive commit messages:
- `Add: new vision_annotate tool`
- `Fix: similarity search edge case with empty store`
- `Update: improve CLIP embedding performance`
- `Docs: add Claude Desktop configuration guide`

## Pull Request Guidelines

- Keep PRs focused — one feature or fix per PR
- Include tests for new functionality
- Update documentation if needed
- Ensure all tests pass before submitting
- Write a clear PR description

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
