#!/usr/bin/env python3
# Copyright 2026 Cortex Contributors
# SPDX-License-Identifier: Apache-2.0
"""Pathfinding: Find the shortest navigation route through a site.

Maps a site and uses Dijkstra's algorithm on the binary graph to find
the shortest path from the homepage to a target page type. This is the
core value proposition: agents compute navigation in microseconds
instead of exploring page-by-page.

Usage:
    pip install cortex-agent
    python 03_pathfinding.py
"""
import cortex_client


PAGE_CHECKOUT = 9  # Checkout / payment page type


def main() -> None:
    # Map a site
    site = cortex_client.map("example.com")
    print(f"Mapped {site.domain}: {site.node_count} nodes, {site.edge_count} edges\n")

    # Find a checkout page (or any target page type)
    targets = site.filter(page_type=PAGE_CHECKOUT, limit=1)
    if not targets:
        # Fall back to any page deep in the site
        targets = site.filter(limit=1, sort_by=(3, "desc"))  # Sort by depth

    if not targets:
        print("No target pages found.")
        return

    target = targets[0]
    print(f"Target: [{target.page_type}] {target.url}")

    # Pathfind from homepage (node 0) to target
    path = site.pathfind(0, target.index)
    if path is None:
        print("No path found between homepage and target.")
        return

    print(f"\nShortest path: {path.hops} hops, weight={path.total_weight:.2f}")
    print(f"Actions required: {len(path.required_actions)}")
    print("\nRoute:")
    for i, node_idx in enumerate(path.nodes):
        pages = site.filter(limit=1)  # Get node info
        prefix = "  → " if i > 0 else "  🏠 "
        print(f"{prefix}Node {node_idx}")

    if path.required_actions:
        print("\nRequired actions along the path:")
        for action in path.required_actions:
            print(f"  - OpCode {action}")


if __name__ == "__main__":
    main()
