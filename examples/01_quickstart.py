#!/usr/bin/env python3
# Copyright 2026 Cortex Contributors
# SPDX-License-Identifier: Apache-2.0
"""Quickstart: Map a website and explore it in 10 lines.

This is the simplest possible Cortex example. It maps a domain,
prints the graph summary, and lists the first few pages found.

Usage:
    pip install cortex-agent
    python 01_quickstart.py
"""
import cortex_client


def main() -> None:
    # Map a domain — auto-starts the Cortex daemon if needed
    site = cortex_client.map("example.com")
    print(f"Mapped {site.domain}: {site.node_count} nodes, {site.edge_count} edges")

    # List all discovered pages
    pages = site.filter(limit=20)
    for page in pages:
        print(f"  [{page.page_type}] {page.url} (confidence={page.confidence:.2f})")


if __name__ == "__main__":
    main()
