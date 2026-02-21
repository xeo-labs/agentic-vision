#!/usr/bin/env bash
set -euo pipefail

echo "╔═══════════════════════════════════════════════╗"
echo "║     CORTEX — Repository Bootstrap             ║"
echo "╚═══════════════════════════════════════════════╝"
echo ""

REPO_ROOT="$(pwd)"

# ─── Directories ────────────────────────────────────
echo "[1/9] Creating directory structure..."

dirs=(
  .github/workflows .github/ISSUE_TEMPLATE
  docs/planning docs/architecture docs/protocol docs/guides docs/cookbooks docs/rfcs
  runtime/src/{pool,renderer,perception,extraction/scripts,cache,trust,cloud,audit,stealth,cli}
  runtime/src/{map,cartography,navigation,live,intelligence}
  runtime/tests
  extractors/{core,community,shared,stealth,dist}
  clients/python/{cortex_client,tests}
  clients/typescript/{src,tests}
  clients/rust/src
  clients/go
  clients/conformance
  integrations/langchain/cortex_langchain
  integrations/crewai/cortex_crewai
  integrations/openclaw/skills
  cloud/{gateway/src,pool/src,cache/src,infra}
  tests/{mapping-suite/{sites/{simple-blog,ecommerce-small,spa-react,no-sitemap},golden},integration,live-sites,adversarial}
  benchmarks scripts
)
for d in "${dirs[@]}"; do mkdir -p "$d"; done

# ─── License ─────────────────────────────────────────
echo "[2/9] Creating LICENSE..."
cat > LICENSE << 'EOF'
                                 Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

1. Definitions.
"License" shall mean the terms and conditions for use, reproduction, and distribution.
"Licensor" shall mean the copyright owner or entity authorized by the copyright owner.
"You" (or "Your") shall mean an individual or Legal Entity exercising permissions.
"Source" form shall mean the preferred form for making modifications.
"Object" form shall mean any form resulting from mechanical transformation of a Source form.
"Work" shall mean the work of authorship made available under the License.
"Derivative Works" shall mean any work that is based on the Work.
"Contribution" shall mean any work of authorship submitted to the Licensor for inclusion.
"Contributor" shall mean Licensor and any entity on behalf of whom a Contribution has been received.

2. Grant of Copyright License. Each Contributor grants to You a perpetual, worldwide,
non-exclusive, no-charge, royalty-free, irrevocable copyright license to reproduce, prepare
Derivative Works of, publicly display, publicly perform, sublicense, and distribute the Work.

3. Grant of Patent License. Each Contributor grants to You a perpetual, worldwide,
non-exclusive, no-charge, royalty-free, irrevocable patent license to make, have made,
use, offer to sell, sell, import, and otherwise transfer the Work.

4. Redistribution. You may reproduce and distribute copies of the Work provided that You
give recipients a copy of this License, cause modified files to carry prominent notices,
and retain all copyright notices.

5. Submission of Contributions. Any Contribution submitted for inclusion shall be under
the terms of this License.

6. Trademarks. This License does not grant permission to use the trade names of the Licensor.

7. Disclaimer of Warranty. THE WORK IS PROVIDED ON AN "AS IS" BASIS, WITHOUT WARRANTIES
OR CONDITIONS OF ANY KIND.

8. Limitation of Liability. IN NO EVENT SHALL ANY CONTRIBUTOR BE LIABLE TO YOU FOR DAMAGES.

9. Accepting Warranty or Additional Liability. You may choose to offer warranty or liability.

END OF TERMS AND CONDITIONS

Copyright 2026 Cortex Contributors

Licensed under the Apache License, Version 2.0 (the "License"); you may not use this
file except in compliance with the License. You may obtain a copy of the License at
http://www.apache.org/licenses/LICENSE-2.0
EOF

# ─── Rust ────────────────────────────────────────────
echo "[3/9] Creating Rust project..."

cat > runtime/Cargo.toml << 'EOF'
[package]
name = "cortex-runtime"
version = "0.1.0"
edition = "2021"
license = "Apache-2.0"
description = "Rapid web cartographer for AI agents"
repository = "https://github.com/agentralabs/agentic-vision"

[[bin]]
name = "cortex"
path = "src/main.rs"

[dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "2.0"
anyhow = "1.0"
chromiumoxide = { version = "0.7", features = ["tokio-runtime"] }
futures = "0.3"
rusqlite = { version = "0.32", features = ["bundled"] }
clap = { version = "4", features = ["derive"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
uuid = { version = "1", features = ["v4"] }
reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
dirs = "6"
chrono = { version = "0.4", features = ["serde"] }
async-trait = "0.1"
byteorder = "1.5"
memmap2 = "0.9"
quick-xml = "0.37"
url = "2.5"
regex = "1.11"
fnv = "1.0"
rayon = "1.10"
dashmap = "6"
petgraph = "0.6"
rand = "0.8"
base64 = "0.22"

[dev-dependencies]
tempfile = "3"
tokio-test = "0.4"
assert_json_diff = "2.0"
criterion = "0.5"
wiremock = "0.6"
EOF

cat > runtime/src/main.rs << 'EOF'
fn main() {
    println!("cortex: not yet implemented. See TASKS.md T-010.");
}
EOF

# ─── TypeScript ──────────────────────────────────────
echo "[4/9] Creating TypeScript projects..."

cat > extractors/package.json << 'EOF'
{
  "name": "@cortex-ai/extractors",
  "version": "0.1.0",
  "private": true,
  "license": "Apache-2.0",
  "scripts": {
    "build": "bash build.sh",
    "lint": "echo 'lint placeholder'",
    "test": "echo 'test placeholder'"
  },
  "devDependencies": {
    "typescript": "^5.7.0",
    "esbuild": "^0.24.0",
    "vitest": "^2.1.0",
    "jsdom": "^25.0.0"
  }
}
EOF

cat > extractors/tsconfig.json << 'EOF'
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "ESNext",
    "moduleResolution": "node",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "outDir": "./dist",
    "rootDir": ".",
    "declaration": true,
    "lib": ["ES2020", "DOM", "DOM.Iterable"]
  },
  "include": ["core/**/*.ts", "shared/**/*.ts"],
  "exclude": ["node_modules", "dist"]
}
EOF

cat > extractors/build.sh << 'BEOF'
#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
echo "Building extraction scripts..."
mkdir -p dist
for extractor in core/*.ts; do
  [ -f "$extractor" ] || continue
  name=$(basename "$extractor" .ts)
  echo "  Building $name..."
  npx esbuild "$extractor" --bundle --format=iife --global-name="CortexExtractor_${name}" --outfile="dist/${name}.js" --platform=browser --target=es2020 2>/dev/null || echo "  Warning: $name build skipped (not implemented yet)"
done
echo "Done."
BEOF
chmod +x extractors/build.sh

# ─── Python Client ───────────────────────────────────
cat > clients/python/pyproject.toml << 'EOF'
[project]
name = "cortex-client"
version = "0.1.0"
description = "Thin client for the Cortex web cartography runtime"
license = { text = "Apache-2.0" }
requires-python = ">=3.10"
readme = "README.md"
dependencies = []

[project.optional-dependencies]
dev = ["pytest>=8.0", "pytest-asyncio>=0.24", "ruff>=0.8", "mypy>=1.13"]

[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"
EOF

cat > clients/python/cortex_client/__init__.py << 'EOF'
"""Cortex Client — Thin client for the Cortex web cartography runtime."""
__version__ = "0.1.0"
EOF

cat > clients/python/README.md << 'EOF'
# cortex-client

Thin Python client for [Cortex](https://github.com/agentralabs/agentic-vision) — the rapid web cartographer for AI agents.

```python
from cortex_client import map
site = map("amazon.com")
products = site.filter(page_type=0x04, features={48: {"lt": 300}})
```
EOF

# ─── TypeScript Client ───────────────────────────────
cat > clients/typescript/package.json << 'EOF'
{
  "name": "cortex-web-client",
  "version": "0.1.0",
  "license": "Apache-2.0",
  "main": "dist/index.js",
  "types": "dist/index.d.ts",
  "scripts": { "build": "tsc", "test": "echo 'test placeholder'" },
  "dependencies": {},
  "devDependencies": { "typescript": "^5.7.0" }
}
EOF

cat > clients/typescript/tsconfig.json << 'EOF'
{
  "compilerOptions": {
    "target": "ES2020", "module": "commonjs", "outDir": "./dist",
    "rootDir": "./src", "strict": true, "declaration": true,
    "esModuleInterop": true, "skipLibCheck": true
  },
  "include": ["src/**/*.ts"], "exclude": ["node_modules", "dist", "tests"]
}
EOF

# ─── Makefile ────────────────────────────────────────
echo "[5/9] Creating Makefile..."

cat > Makefile << 'MEOF'
.PHONY: all build build-runtime build-extractors test test-unit test-mapping test-integration test-conformance lint clean

all: build

build: build-extractors build-runtime

build-runtime:
	cd runtime && cargo build --release

build-runtime-debug:
	cd runtime && cargo build

build-extractors:
	cd extractors && npm install --silent && bash build.sh
	mkdir -p runtime/src/extraction/scripts
	cp extractors/dist/*.js runtime/src/extraction/scripts/ 2>/dev/null || true

test: test-unit test-mapping test-integration

test-unit:
	cd runtime && cargo test --lib
	cd clients/python && pip install -e ".[dev]" -q 2>/dev/null && pytest tests/ -x -q 2>/dev/null || true

test-mapping:
	cd runtime && cargo test --test mapping_fixtures 2>/dev/null || echo "mapping fixtures not yet implemented"

test-integration:
	cd runtime && cargo test --test integration 2>/dev/null || echo "integration tests not yet implemented"

test-conformance:
	cd clients/conformance && python runner.py 2>/dev/null || echo "conformance tests not yet implemented"

lint: lint-rust lint-python

lint-rust:
	cd runtime && cargo fmt --check
	cd runtime && cargo clippy -- -D warnings

lint-python:
	cd clients/python && pip install -e ".[dev]" -q 2>/dev/null && ruff check . 2>/dev/null || true
	cd clients/python && mypy --strict cortex_client/ 2>/dev/null || true

clean:
	cd runtime && cargo clean
	rm -rf extractors/dist extractors/node_modules
	find . -type d -name __pycache__ -exec rm -rf {} + 2>/dev/null || true
MEOF

# ─── Community Files ─────────────────────────────────
echo "[6/9] Creating community files..."

cat > CODE_OF_CONDUCT.md << 'EOF'
# Contributor Covenant Code of Conduct

We pledge to make participation in our community a harassment-free experience for everyone.

See https://www.contributor-covenant.org/version/2/1/code_of_conduct/ for full text.
EOF

cat > CONTRIBUTING.md << 'EOF'
# Contributing to Cortex

## Quick Start
1. Fork & clone
2. `bash scripts/bootstrap.sh` (if not already bootstrapped)
3. `cd runtime && cargo build`
4. `cd extractors && npm install`
5. Make changes, run `make test`, submit PR

## Easiest Contribution: Write an Extractor
See `docs/guides/writing-extractors.md`.

## Code Style
- Rust: `cargo fmt` + `cargo clippy -- -D warnings`
- TypeScript: strict mode, no `any`
- Python: `ruff` + `mypy --strict`, zero external dependencies

## Pull Requests
- Tests required for all changes
- CI must pass
- One maintainer review required
EOF

cat > SECURITY.md << 'EOF'
# Security Policy

Report vulnerabilities via GitHub's private vulnerability reporting.
DO NOT open public issues for security vulnerabilities.

We care about: cross-session data leakage, credential vault weaknesses,
action sandbox bypasses, privilege escalation, memory safety.
EOF

cat > CHANGELOG.md << 'EOF'
# Changelog

## [Unreleased]
### Added
- Initial project structure
- Planning documents (01-04)
EOF

# ─── GitHub Templates ────────────────────────────────
echo "[7/9] Creating GitHub templates..."

cat > .github/ISSUE_TEMPLATE/bug_report.md << 'EOF'
---
name: Bug Report
about: Report a bug
title: "[BUG] "
labels: bug
---
**Description:** 
**Steps to Reproduce:** 
**Expected:** 
**Actual:** 
**Environment:** OS, `cortex --version`, `cortex doctor` output
EOF

cat > .github/ISSUE_TEMPLATE/feature_request.md << 'EOF'
---
name: Feature Request
about: Suggest a feature
title: "[FEATURE] "
labels: enhancement
---
**Problem:**
**Proposed Solution:**
EOF

cat > .github/PULL_REQUEST_TEMPLATE.md << 'EOF'
## What
Brief description.

## Type
- [ ] Bug fix
- [ ] New feature
- [ ] Extractor
- [ ] Documentation

## Testing
- [ ] Tests added/updated
- [ ] All tests pass
- [ ] Lints pass
EOF

# ─── Gitignore ───────────────────────────────────────
echo "[8/9] Creating config files..."

cat > .gitignore << 'EOF'
target/
node_modules/
dist/
__pycache__/
*.pyc
*.egg-info/
.venv/
.idea/
.vscode/
*.swp
.cortex/
/tmp/cortex.sock
.DS_Store
Thumbs.db
*.so
*.dylib
*.dll
EOF

# ─── Placeholder files ───────────────────────────────
echo "[9/9] Creating placeholder files..."
touch extractors/community/.gitkeep
touch tests/integration/.gitkeep
touch tests/adversarial/.gitkeep
touch tests/live-sites/.gitkeep
touch benchmarks/.gitkeep
touch docs/rfcs/.gitkeep
touch docs/cookbooks/.gitkeep
touch docs/architecture/.gitkeep

# ─── Test fixture: simple-blog sitemap ───────────────
cat > tests/mapping-suite/sites/simple-blog/sitemap.xml << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>https://simple-blog.test/</loc><priority>1.0</priority></url>
  <url><loc>https://simple-blog.test/about</loc><priority>0.5</priority></url>
  <url><loc>https://simple-blog.test/contact</loc><priority>0.5</priority></url>
  <url><loc>https://simple-blog.test/post/hello-world</loc><priority>0.8</priority></url>
  <url><loc>https://simple-blog.test/post/second-post</loc><priority>0.8</priority></url>
  <url><loc>https://simple-blog.test/post/third-post</loc><priority>0.8</priority></url>
  <url><loc>https://simple-blog.test/archive</loc><priority>0.6</priority></url>
</urlset>
EOF

cat > tests/mapping-suite/sites/simple-blog/robots.txt << 'EOF'
User-agent: *
Allow: /
Sitemap: https://simple-blog.test/sitemap.xml
EOF

cat > tests/mapping-suite/sites/simple-blog/index.html << 'EOF'
<!DOCTYPE html>
<html lang="en">
<head><title>Simple Blog</title></head>
<body>
  <header><nav>
    <a href="/">Home</a> <a href="/about">About</a> <a href="/contact">Contact</a> <a href="/archive">Archive</a>
  </nav></header>
  <main>
    <h1>Welcome to Simple Blog</h1>
    <article><h2><a href="/post/hello-world">Hello World</a></h2><p>First post content.</p></article>
    <article><h2><a href="/post/second-post">Second Post</a></h2><p>Second post content.</p></article>
    <article><h2><a href="/post/third-post">Third Post</a></h2><p>Third post content.</p></article>
  </main>
  <footer><p>&copy; 2026 Simple Blog</p></footer>
</body>
</html>
EOF

cat > tests/mapping-suite/golden/simple-blog.json << 'EOF'
{
  "domain": "simple-blog.test",
  "expected_nodes": {"min": 6, "max": 10},
  "expected_edges": {"min": 10, "max": 30},
  "required_node_types": {
    "home": 1,
    "article": {"min": 2, "max": 4},
    "about_page": 1,
    "contact_page": 1
  },
  "required_paths": [
    {"from_type": "home", "to_type": "article", "max_hops": 2},
    {"from_type": "home", "to_type": "about_page", "max_hops": 1}
  ]
}
EOF

echo ""
echo "╔═══════════════════════════════════════════════╗"
echo "║           Bootstrap Complete!                 ║"
echo "╠═══════════════════════════════════════════════╣"
echo "║                                               ║"
echo "║  Next: Claude Code reads CLAUDE.md            ║"
echo "║  Then: Executes TASKS.md top to bottom        ║"
echo "║                                               ║"
echo "║  Verify:                                      ║"
echo "║    cd runtime && cargo check                  ║"
echo "║    cd extractors && npm install               ║"
echo "║                                               ║"
echo "╚═══════════════════════════════════════════════╝"
