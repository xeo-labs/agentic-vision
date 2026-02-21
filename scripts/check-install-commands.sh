#!/usr/bin/env bash
set -euo pipefail

fail() {
  echo "ERROR: $*" >&2
  exit 1
}

assert_contains() {
  local pattern="$1"
  shift
  if ! rg -nF "$pattern" "$@" >/dev/null; then
    fail "Missing required install command: ${pattern}"
  fi
}

# Front-facing command requirements
assert_contains "curl -fsSL https://agentralabs.tech/install/vision | bash" README.md docs/quickstart.md
assert_contains "cargo install agentic-vision-mcp" README.md docs/quickstart.md
assert_contains "cargo add agentic-vision" README.md docs/quickstart.md

# Invalid patterns
if rg -n "cargo install agentic-vision agentic-vision-mcp" README.md docs -g '*.md' >/dev/null; then
  fail "Found invalid combined cargo install for vision library + MCP"
fi
if rg -n "^cargo install agentic-vision$" README.md docs -g '*.md' >/dev/null; then
  fail "Found invalid binary install command for library crate agentic-vision"
fi

# Installer health
bash -n scripts/install.sh
bash scripts/install.sh --dry-run >/dev/null

# Public endpoint/package health
curl -fsSL https://agentralabs.tech/install/vision >/dev/null
curl -fsSL https://crates.io/api/v1/crates/agentic-vision >/dev/null
curl -fsSL https://crates.io/api/v1/crates/agentic-vision-mcp >/dev/null

echo "Install command guardrails passed (vision)."
