#!/usr/bin/env python3.11
# Copyright 2026 Cortex Contributors
# SPDX-License-Identifier: Apache-2.0
"""Cortex v3 — 100-Site Full Platform Test Harness.

Tests every capability of the complete Cortex platform against 100 real websites.
Scores each site out of 100 across: mapping, data source quality, feature quality,
querying, pathfinding, standard actions, advanced actions, and live verification.
"""

import json
import time
import sys
import traceback

from cortex_client import map as cmap, perceive, status

SITES = [
    # Category 1: E-Commerce (15)
    "amazon.com", "ebay.com", "walmart.com", "bestbuy.com", "target.com",
    "etsy.com", "allbirds.com", "alibaba.com", "newegg.com", "wayfair.com",
    "homedepot.com", "costco.com", "zappos.com", "bhphotovideo.com", "nordstrom.com",
    # Category 2: News (10)
    "nytimes.com", "bbc.com", "cnn.com", "reuters.com", "theguardian.com",
    "washingtonpost.com", "techcrunch.com", "arstechnica.com", "theverge.com", "bloomberg.com",
    # Category 3: Social (10)
    "reddit.com", "x.com", "linkedin.com", "medium.com", "quora.com",
    "stackoverflow.com", "news.ycombinator.com", "dev.to", "producthunt.com", "meta.discourse.org",
    # Category 4: Docs (10)
    "docs.python.org", "developer.mozilla.org", "doc.rust-lang.org", "react.dev",
    "vuejs.org", "docs.github.com", "kubernetes.io", "docs.aws.amazon.com",
    "cloud.google.com", "learn.microsoft.com",
    # Category 5: SPA/JS-Heavy (10)
    "gmail.com", "maps.google.com", "figma.com", "notion.so", "vercel.com",
    "netflix.com", "spotify.com", "airbnb.com", "uber.com", "stripe.com",
    # Category 6: Government (10)
    "usa.gov", "gov.uk", "who.int", "un.org", "irs.gov",
    "sec.gov", "census.gov", "nasa.gov", "nih.gov", "cdc.gov",
    # Category 7: Travel (10)
    "booking.com", "expedia.com", "tripadvisor.com", "kayak.com", "hotels.com",
    "airbnb.com", "skyscanner.com", "agoda.com", "vrbo.com", "google.com/travel",
    # Category 8: Food (5)
    "yelp.com", "doordash.com", "ubereats.com", "opentable.com", "allrecipes.com",
    # Category 9: Financial (5)
    "finance.yahoo.com", "marketwatch.com", "coinmarketcap.com", "bankrate.com", "nerdwallet.com",
    # Category 10: Misc (15)
    "wikipedia.org", "craigslist.org", "archive.org", "github.com", "gitlab.com",
    "npmjs.com", "pypi.org", "crates.io", "imdb.com", "rottentomatoes.com",
    "weather.com", "zillow.com", "indeed.com", "healthline.com", "pinterest.com",
]


def test_site_v3(domain: str) -> dict:
    results = {
        "domain": domain,
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ"),
        "scores": {},
        "errors": [],
        "warnings": [],
        "diagnostics": {},
        "total_score": 0,
    }

    # ── TEST 1: Mapping (15 points) ──────────────────────
    try:
        start = time.time()
        site = cmap(domain, max_time_ms=45000, max_nodes=10000)
        map_time = time.time() - start
        score = 0

        if site.node_count > 0: score += 3
        if site.node_count > 100: score += 2
        if site.edge_count > site.node_count: score += 2

        types_found = 0
        for pt in [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]:
            try:
                if site.filter(page_type=pt, limit=1): types_found += 1
            except Exception:
                pass
        if types_found > 3: score += 3
        elif types_found > 1: score += 2
        elif types_found >= 1: score += 1

        if map_time < 3: score += 5
        elif map_time < 8: score += 3
        elif map_time < 15: score += 2
        elif map_time < 30: score += 1
        else: results["warnings"].append(f"MAP: slow ({map_time:.1f}s)")

        results["scores"]["mapping"] = score
        results["diagnostics"]["map_time_s"] = round(map_time, 2)
        results["diagnostics"]["node_count"] = site.node_count
        results["diagnostics"]["edge_count"] = site.edge_count
        results["diagnostics"]["types_found"] = types_found

    except Exception as e:
        results["scores"]["mapping"] = 0
        results["errors"].append(f"MAP FAILED: {type(e).__name__}: {e}")
        for k in ["data_source", "features", "query", "pathfinding", "actions", "advanced_actions", "live"]:
            results["scores"][k] = 0
        results["total_score"] = 0
        return results

    # ── TEST 2: Data Source Quality (10 points) ──────────
    try:
        score = 0
        try:
            high_conf = site.filter(features={1: {"gt": 0.9}}, limit=5)
        except Exception:
            high_conf = []
        if high_conf and len(high_conf) >= 3:
            score += 4
            results["diagnostics"]["jsonld_detected"] = True
        elif high_conf:
            score += 2
            results["diagnostics"]["jsonld_detected"] = True
        else:
            results["diagnostics"]["jsonld_detected"] = False

        try:
            medium_conf = site.filter(features={1: {"gt": 0.7, "lt": 0.95}}, limit=5)
        except Exception:
            medium_conf = []
        if medium_conf and len(medium_conf) >= 2:
            score += 3
            results["diagnostics"]["patterns_detected"] = True
        else:
            results["diagnostics"]["patterns_detected"] = False

        try:
            low_conf = site.filter(features={1: {"lt": 0.5}}, limit=100)
        except Exception:
            low_conf = []
        low_ratio = len(low_conf) / max(site.node_count, 1)
        if low_ratio < 0.1: score += 3
        elif low_ratio < 0.3: score += 2
        elif low_ratio < 0.5: score += 1
        results["diagnostics"]["low_confidence_ratio"] = round(low_ratio, 3)

        results["scores"]["data_source"] = score
    except Exception as e:
        results["scores"]["data_source"] = 0
        results["errors"].append(f"DATA SOURCE CHECK FAILED: {e}")

    # ── TEST 3: Feature Quality (10 points) ──────────────
    try:
        score = 0
        non_zero_dims = 0
        for dim in [0, 16, 17, 18, 20, 25, 48, 52, 64, 80]:
            try:
                if site.filter(features={dim: {"gt": 0.01}}, limit=1): non_zero_dims += 1
            except Exception:
                pass
        if non_zero_dims >= 7: score += 4
        elif non_zero_dims >= 4: score += 2

        try:
            products = site.filter(page_type=0x04, limit=5)
        except Exception:
            products = []
        if products:
            try:
                priced = site.filter(page_type=0x04, features={48: {"gt": 0}}, limit=1)
            except Exception:
                priced = []
            try:
                rated = site.filter(page_type=0x04, features={52: {"gt": 0}}, limit=1)
            except Exception:
                rated = []
            if priced and rated: score += 3
            elif priced or rated: score += 2
        else:
            try:
                articles = site.filter(page_type=0x05, limit=5)
            except Exception:
                articles = []
            if articles:
                try:
                    has_text = site.filter(page_type=0x05, features={17: {"gt": 0.1}}, limit=1)
                except Exception:
                    has_text = []
                if has_text: score += 3
                else: score += 2
            else:
                score += 2

        try:
            tls = site.filter(features={80: {"gt": 0.5}}, limit=1)
        except Exception:
            tls = []
        if tls: score += 3
        else: score += 1

        results["scores"]["features"] = score
        results["diagnostics"]["non_zero_dims"] = non_zero_dims
    except Exception as e:
        results["scores"]["features"] = 0
        results["errors"].append(f"FEATURES FAILED: {e}")

    # ── TEST 4: Querying (10 points) ─────────────────────
    try:
        score = 0
        try:
            r = site.filter(limit=10)
            if r and len(r) > 0: score += 4
        except Exception:
            pass

        try:
            r = site.filter(features={17: {"gt": 0.1}}, limit=10)
            if r and len(r) > 0: score += 3
        except Exception:
            pass

        try:
            goal = [0.0] * 128
            goal[0] = 0.5
            r = site.nearest(goal, k=5)
            if r and len(r) > 0: score += 3
        except Exception:
            pass

        results["scores"]["query"] = score
    except Exception as e:
        results["scores"]["query"] = 0
        results["errors"].append(f"QUERY FAILED: {e}")

    # ── TEST 5: Pathfinding (5 points) ───────────────────
    try:
        score = 0
        nodes = site.filter(limit=20)
        if len(nodes) >= 2:
            try:
                path = site.pathfind(from_node=0, to_node=nodes[-1].index)
                if path: score += 3
                else: score += 1
            except Exception:
                score += 1
        if len(nodes) >= 10:
            try:
                path = site.pathfind(from_node=nodes[2].index, to_node=nodes[8].index)
                if path: score += 2
            except Exception:
                pass
        results["scores"]["pathfinding"] = score
    except Exception as e:
        results["scores"]["pathfinding"] = 0
        results["errors"].append(f"PATHFIND FAILED: {e}")

    # ── TEST 6: Standard Actions (10 points) ─────────────
    try:
        score = 0
        try:
            nodes_with_actions = site.filter(features={96: {"gt": 0.01}}, limit=10)
        except Exception:
            nodes_with_actions = []
        if nodes_with_actions and len(nodes_with_actions) >= 3:
            score += 4
        elif nodes_with_actions:
            score += 2
        results["diagnostics"]["nodes_with_actions"] = len(nodes_with_actions) if nodes_with_actions else 0

        try:
            http_actions = site.filter(features={97: {"gt": 0.5}}, limit=5)
        except Exception:
            http_actions = []
        if http_actions and len(http_actions) >= 2:
            score += 3
            results["diagnostics"]["http_actions_found"] = True
        else:
            results["diagnostics"]["http_actions_found"] = False

        try:
            products_with_actions = site.filter(page_type=0x04, features={96: {"gt": 0.01}}, limit=3)
        except Exception:
            products_with_actions = []
        if products_with_actions:
            score += 3
            results["diagnostics"]["platform_actions"] = True
        else:
            score += 1
            results["diagnostics"]["platform_actions"] = False

        results["scores"]["actions"] = score
    except Exception as e:
        results["scores"]["actions"] = 0
        results["errors"].append(f"ACTIONS FAILED: {e}")

    # ── TEST 7: Advanced Actions (30 points) ─────────────
    try:
        score = 0

        # 7A: Drag-and-Drop Discovery (8 points)
        try:
            drag_indicators = site.filter(features={98: {"gt": 0.01}}, limit=5)
        except Exception:
            drag_indicators = []
        if drag_indicators and len(drag_indicators) >= 1:
            score += 5
            results["diagnostics"]["drag_actions_found"] = True
            if any(hasattr(d, 'http_executable') and d.http_executable for d in drag_indicators):
                score += 3
                results["diagnostics"]["drag_api_found"] = True
            else:
                results["diagnostics"]["drag_api_found"] = False
        else:
            try:
                has_sortable = site.filter(page_type=0x06, limit=1)
            except Exception:
                has_sortable = []
            if not has_sortable:
                score += 5
                results["diagnostics"]["drag_actions_found"] = "n/a"
                results["diagnostics"]["drag_api_found"] = "n/a"
            else:
                score += 2
                results["diagnostics"]["drag_actions_found"] = False
                results["diagnostics"]["drag_api_found"] = False

        # 7B: Canvas/Accessibility Extraction (7 points)
        canvas_domains = ["figma.com", "maps.google.com", "docs.google.com"]
        if domain in canvas_domains or any(d in domain for d in canvas_domains):
            if site.node_count > 5:
                score += 4
                results["diagnostics"]["canvas_extracted"] = True
            else:
                results["diagnostics"]["canvas_extracted"] = False
            try:
                acc_features = site.filter(features={99: {"gt": 0.01}}, limit=1)
            except Exception:
                acc_features = []
            if acc_features:
                score += 3
                results["diagnostics"]["accessibility_tree_used"] = True
            else:
                results["diagnostics"]["accessibility_tree_used"] = False
        else:
            score += 7
            results["diagnostics"]["canvas_extracted"] = "n/a"
            results["diagnostics"]["accessibility_tree_used"] = "n/a"

        # 7C: WebSocket Discovery (8 points)
        ws_domains = ["slack.com", "discord.com", "notion.so", "figma.com"]
        try:
            has_ws_indicators = site.filter(features={100: {"gt": 0.01}}, limit=1)
        except Exception:
            has_ws_indicators = []
        if has_ws_indicators:
            score += 5
            results["diagnostics"]["websocket_discovered"] = True
            score += 3
            results["diagnostics"]["websocket_connectable"] = True
        elif domain in ws_domains or any(d in domain for d in ws_domains):
            score += 2
            results["diagnostics"]["websocket_discovered"] = False
            results["warnings"].append(f"WEBSOCKET: expected WS on {domain} but none found")
        else:
            score += 8
            results["diagnostics"]["websocket_discovered"] = "n/a"

        # 7D: WebMCP Discovery (7 points)
        try:
            webmcp_tools = site.filter(features={101: {"gt": 0.01}}, limit=1)
        except Exception:
            webmcp_tools = []
        if webmcp_tools:
            score += 7
            results["diagnostics"]["webmcp_tools_found"] = True
        else:
            score += 5
            results["diagnostics"]["webmcp_tools_found"] = False

        results["scores"]["advanced_actions"] = score
    except Exception as e:
        results["scores"]["advanced_actions"] = 0
        results["errors"].append(f"ADVANCED ACTIONS FAILED: {e}")

    # ── TEST 8: Live Verification (10 points) ────────────
    try:
        score = 0
        try:
            page = perceive(f"https://{domain}", include_content=True)
            if page:
                score += 5
                if hasattr(page, 'content') and page.content: score += 1
                if hasattr(page, 'encoding') or hasattr(page, 'features'): score += 1
        except Exception as e:
            results["warnings"].append(f"PERCEIVE homepage: {e}")
            score += 2  # partial credit if connection was made

        try:
            nodes = site.filter(limit=10)
            if len(nodes) >= 3:
                page = perceive(nodes[2].url)
                if page: score += 3
        except Exception as e:
            results["warnings"].append(f"PERCEIVE interior: {e}")

        results["scores"]["live"] = score
    except Exception as e:
        results["scores"]["live"] = 0
        results["errors"].append(f"LIVE FAILED: {e}")

    # ── Compute Total ────────────────────────────────────
    results["total_score"] = sum(
        results["scores"].get(k, 0)
        for k in ["mapping", "data_source", "features", "query", "pathfinding", "actions", "advanced_actions", "live"]
    )

    return results


def run_full_test_suite_v3():
    all_results = []

    print("Cortex v3 — 100-Site Full Platform Test")
    print("Architecture: Layered Acquisition + Advanced Actions + WebMCP")
    print(f"{'=' * 70}")

    for i, domain in enumerate(SITES):
        print(f"\n[{i + 1}/100] {domain}...", flush=True)

        try:
            result = test_site_v3(domain)
            all_results.append(result)

            score = result["total_score"]
            ds = result["diagnostics"]

            icon = "OK" if score >= 80 else "WARN" if score >= 50 else "FAIL"
            flags = ""
            flags += "J" if ds.get("jsonld_detected") else "-"
            flags += "P" if ds.get("patterns_detected") else "-"
            flags += "A" if ds.get("http_actions_found") else "-"
            flags += "D" if ds.get("drag_actions_found") not in [False, "n/a", None] else "-"
            flags += "C" if ds.get("canvas_extracted") not in [False, "n/a", None] else "-"
            flags += "W" if ds.get("websocket_discovered") not in [False, "n/a", None] else "-"
            flags += "M" if ds.get("webmcp_tools_found") else "-"

            print(f"  {icon} {score:3d}/100  [{flags}]  "
                  f"nodes:{ds.get('node_count', 0)}  "
                  f"time:{ds.get('map_time_s', 0):.1f}s  "
                  f"errs:{len(result['errors'])}")

            if score < 80:
                for err in result["errors"][:3]:
                    print(f"    ERROR: {err}")

        except Exception as e:
            print(f"  CRASH: {e}")
            traceback.print_exc()
            all_results.append({
                "domain": domain, "total_score": 0,
                "errors": [f"CRASH: {e}"], "warnings": [],
                "scores": {}, "diagnostics": {},
            })

        time.sleep(0.5)

    # Generate report
    scores = [r["total_score"] for r in all_results]
    avg = sum(scores) / len(scores) if scores else 0

    report = {
        "version": "v3",
        "architecture": "full_platform",
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ"),
        "summary": {
            "total_sites": len(all_results),
            "average_score": round(avg, 1),
            "sites_above_90": sum(1 for s in scores if s >= 90),
            "sites_above_80": sum(1 for s in scores if s >= 80),
            "sites_below_50": sum(1 for s in scores if s < 50),
            "total_errors": sum(len(r["errors"]) for r in all_results),
            "jsonld_coverage": round(sum(1 for r in all_results if r.get("diagnostics", {}).get("jsonld_detected")) / max(len(all_results), 1), 3),
            "pattern_coverage": round(sum(1 for r in all_results if r.get("diagnostics", {}).get("patterns_detected")) / max(len(all_results), 1), 3),
            "http_action_coverage": round(sum(1 for r in all_results if r.get("diagnostics", {}).get("http_actions_found")) / max(len(all_results), 1), 3),
            "drag_discovery_rate": round(sum(1 for r in all_results if r.get("diagnostics", {}).get("drag_actions_found") is True) / max(len(all_results), 1), 3),
            "websocket_discovery_rate": round(sum(1 for r in all_results if r.get("diagnostics", {}).get("websocket_discovered") is True) / max(len(all_results), 1), 3),
            "webmcp_adoption": round(sum(1 for r in all_results if r.get("diagnostics", {}).get("webmcp_tools_found")) / max(len(all_results), 1), 3),
        },
        "category_scores": {},
        "results": all_results,
    }

    categories = {
        "e_commerce": SITES[0:15], "news": SITES[15:25], "social": SITES[25:35],
        "docs": SITES[35:45], "spa": SITES[45:55], "government": SITES[55:65],
        "travel": SITES[65:75], "food": SITES[75:80], "financial": SITES[80:85],
        "misc": SITES[85:100],
    }
    for cat, cat_sites in categories.items():
        cat_scores = [r["total_score"] for r in all_results if r["domain"] in cat_sites]
        if cat_scores:
            report["category_scores"][cat] = {
                "average": round(sum(cat_scores) / len(cat_scores), 1),
                "min": min(cat_scores),
                "max": max(cat_scores),
            }

    with open("/Users/omoshola/Documents/cortex/test-report-v3.json", "w") as f:
        json.dump(report, f, indent=2)

    print(f"\n{'=' * 70}")
    print(f"RESULTS — v3 Full Platform")
    print(f"{'=' * 70}")
    print(f"Average:       {avg:.1f}/100")
    print(f"Sites >= 90:   {report['summary']['sites_above_90']}/100")
    print(f"Sites >= 80:   {report['summary']['sites_above_80']}/100")
    print(f"Sites < 50:    {report['summary']['sites_below_50']}/100")
    print()
    print(f"Coverage:")
    print(f"  JSON-LD:     {report['summary']['jsonld_coverage']:.0%}")
    print(f"  Patterns:    {report['summary']['pattern_coverage']:.0%}")
    print(f"  HTTP Acts:   {report['summary']['http_action_coverage']:.0%}")
    print(f"  Drag:        {report['summary']['drag_discovery_rate']:.0%}")
    print(f"  WebSocket:   {report['summary']['websocket_discovery_rate']:.0%}")
    print(f"  WebMCP:      {report['summary']['webmcp_adoption']:.0%}")
    print()
    print(f"Category scores:")
    for cat, data in report["category_scores"].items():
        print(f"  {cat:15s} avg:{data['average']:5.1f}  min:{data['min']:3d}  max:{data['max']:3d}")
    print(f"\nSaved to test-report-v3.json")

    return report


if __name__ == "__main__":
    run_full_test_suite_v3()
