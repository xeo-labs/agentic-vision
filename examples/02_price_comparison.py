#!/usr/bin/env python3
# Copyright 2026 Cortex Contributors
# SPDX-License-Identifier: Apache-2.0
"""Price comparison across multiple e-commerce sites.

Maps several shopping sites in parallel, extracts product pages,
and ranks them by price. Demonstrates multi-site mapping, feature
filtering, and the commerce dimensions of the 128-d feature vector.

Usage:
    pip install cortex-agent
    python 02_price_comparison.py
"""
import cortex_client


# Feature vector dimensions for commerce data
FEAT_PRICE = 48
FEAT_RATING = 52
FEAT_DISCOUNT = 50

# Page type codes
PAGE_PRODUCT_DETAIL = 4


def main() -> None:
    domains = [
        "amazon.com",
        "ebay.com",
        "walmart.com",
        "bestbuy.com",
        "target.com",
    ]

    print("Mapping e-commerce sites...")
    sites = cortex_client.map_many(domains, max_render=50)

    # Collect product pages with prices across all sites
    all_products: list[dict] = []
    for site in sites:
        products = site.filter(
            page_type=PAGE_PRODUCT_DETAIL,
            features={FEAT_PRICE: {"min": 0.01}},  # Has a price
            sort_by=(FEAT_PRICE, "asc"),
            limit=20,
        )
        for p in products:
            all_products.append({
                "domain": site.domain,
                "url": p.url,
                "price": p.features.get(FEAT_PRICE, 0.0),
                "rating": p.features.get(FEAT_RATING, 0.0),
                "discount": p.features.get(FEAT_DISCOUNT, 0.0),
            })

    # Sort globally by price
    all_products.sort(key=lambda x: x["price"])

    # Print comparison table
    print(f"\n{'Domain':<20} {'Price':>8} {'Rating':>7} {'Disc%':>6}  URL")
    print("-" * 90)
    for p in all_products[:25]:
        print(
            f"{p['domain']:<20} "
            f"${p['price']:>7.2f} "
            f"{p['rating']:>6.1f} "
            f"{p['discount']:>5.0f}%  "
            f"{p['url'][:40]}"
        )

    print(f"\nTotal products found: {len(all_products)} across {len(sites)} sites")


if __name__ == "__main__":
    main()
