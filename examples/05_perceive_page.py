#!/usr/bin/env python3
# Copyright 2026 Cortex Contributors
# SPDX-License-Identifier: Apache-2.0
"""Perceive: Analyze a single page without mapping the full site.

Sometimes you only need information about one URL. cortex_client.perceive()
fetches and analyzes a page, returning its classification, feature vector,
and extracted content — without building a full site graph.

Usage:
    pip install cortex-agent
    python 05_perceive_page.py
"""
import cortex_client


def main() -> None:
    urls = [
        "https://example.com",
        "https://news.ycombinator.com",
        "https://en.wikipedia.org/wiki/Graph_theory",
    ]

    for url in urls:
        print(f"\nPerceiving: {url}")
        result = cortex_client.perceive(url)

        print(f"  Final URL:   {result.final_url}")
        print(f"  Page type:   {result.page_type}")
        print(f"  Confidence:  {result.confidence:.2f}")
        print(f"  Features:    {len(result.features)} dimensions extracted")

        # Print a few notable features
        feature_names = {
            0: "page_type",
            1: "confidence",
            2: "depth",
            3: "load_time_ms",
            16: "text_density",
            17: "heading_count",
            18: "image_count",
            64: "link_count",
            65: "form_count",
            80: "is_https",
        }
        for dim, name in feature_names.items():
            if dim in result.features:
                print(f"    [{dim:>3}] {name}: {result.features[dim]:.2f}")


if __name__ == "__main__":
    main()
