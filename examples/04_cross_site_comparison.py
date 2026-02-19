#!/usr/bin/env python3
# Copyright 2026 Cortex Contributors
# SPDX-License-Identifier: Apache-2.0
"""Cross-site comparison: Compare two or more site structures.

Uses cortex_client.compare() to find structural similarities and
differences between websites. Useful for competitive analysis,
migration planning, or architecture audits.

Usage:
    pip install cortex-agent
    python 04_cross_site_comparison.py
"""
import cortex_client


def main() -> None:
    domains = ["example.com", "iana.org"]

    print(f"Comparing {len(domains)} sites: {', '.join(domains)}\n")

    # Compare maps structurally
    result = cortex_client.compare(domains)

    print(f"Domains analyzed: {result.domains}")
    print(f"Common page types: {result.common_types}")

    # Map each site individually for detailed stats
    for domain in domains:
        site = cortex_client.map(domain)
        print(f"\n--- {domain} ---")
        print(f"  Nodes: {site.node_count}")
        print(f"  Edges: {site.edge_count}")

        # Page type distribution
        type_names = {
            0: "Unknown",
            1: "Homepage",
            2: "ListPage",
            3: "ArticlePage",
            4: "ProductDetail",
            5: "SearchResults",
            6: "AuthPage",
            7: "FormPage",
            8: "MediaPage",
            9: "Checkout",
        }
        for type_code, type_name in type_names.items():
            pages = site.filter(page_type=type_code, limit=1000)
            if pages:
                print(f"  {type_name}: {len(pages)} pages")


if __name__ == "__main__":
    main()
