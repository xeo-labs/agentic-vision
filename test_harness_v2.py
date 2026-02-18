#!/usr/bin/env python3
"""Cortex v2 — 100-site test harness.

Tests: mapping, data source quality, feature quality, querying,
pathfinding, action discovery, and live verification.

Architecture: Layered Acquisition (no-browser mapping).
"""
import sys
import os
import time
import json
import signal
import traceback

# Ensure cortex binary is on PATH for the client's auto-start logic
os.environ["PATH"] = "/Users/omoshola/Documents/cortex/runtime/target/release:" + os.environ.get("PATH", "")

sys.path.insert(0, "/Users/omoshola/Documents/cortex/clients/python")
from cortex_client import map as cortex_map, perceive, status
from cortex_client.errors import CortexError


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
    "airbnb.com", "skyscanner.com", "agoda.com", "vrbo.com", "google.com",
    # Category 8: Food (5)
    "yelp.com", "doordash.com", "ubereats.com", "opentable.com", "allrecipes.com",
    # Category 9: Financial (5)
    "finance.yahoo.com", "marketwatch.com", "coinmarketcap.com", "bankrate.com", "nerdwallet.com",
    # Category 10: Misc (15)
    "wikipedia.org", "craigslist.org", "archive.org", "github.com", "gitlab.com",
    "npmjs.com", "pypi.org", "crates.io", "imdb.com", "rottentomatoes.com",
    "weather.com", "zillow.com", "indeed.com", "healthline.com", "pinterest.com",
]


def test_site_v2(domain: str) -> dict:
    """Run expanded v2 test suite on a single domain."""
    results = {
        "domain": domain,
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ"),
        "scores": {},
        "errors": [],
        "warnings": [],
        "diagnostics": {},
        "total_score": 0,
    }

    # ── TEST 1: Mapping (20 points) ──────────────────────
    try:
        start = time.time()
        site = cortex_map(domain, max_time_ms=30000, max_nodes=10000, max_render=50, timeout_ms=60000)
        map_time = time.time() - start

        score = 0

        # Map returned (4 points)
        score += 4

        # Node count (4 points)
        if site.node_count > 100:
            score += 4
        elif site.node_count > 10:
            score += 2
        elif site.node_count > 0:
            score += 1
        else:
            results["errors"].append("MAP: zero nodes")

        # Edge count (4 points)
        if site.edge_count > site.node_count:
            score += 4
        elif site.edge_count > 0:
            score += 2
        else:
            results["errors"].append("MAP: zero edges")

        # Multiple page types (4 points)
        types_found = 0
        for pt in [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]:
            try:
                if site.filter(page_type=pt, limit=1):
                    types_found += 1
            except Exception:
                pass
        if types_found > 3:
            score += 4
        elif types_found > 1:
            score += 2
        elif types_found >= 1:
            score += 1

        # Speed (4 points) — v2 expects faster (no browser)
        if map_time < 3:
            score += 4
        elif map_time < 8:
            score += 3
        elif map_time < 15:
            score += 2
        elif map_time < 30:
            score += 1
        else:
            results["warnings"].append(f"MAP: slow ({map_time:.1f}s)")

        results["scores"]["mapping"] = score
        results["diagnostics"]["map_time_s"] = round(map_time, 2)
        results["diagnostics"]["node_count"] = site.node_count
        results["diagnostics"]["edge_count"] = site.edge_count
        results["diagnostics"]["types_found"] = types_found

    except Exception as e:
        results["scores"]["mapping"] = 0
        results["errors"].append(f"MAP FAILED: {type(e).__name__}: {str(e)[:200]}")
        results["scores"].update({
            "data_source": 0, "features": 0, "query": 0,
            "pathfinding": 0, "actions": 0, "live": 0,
        })
        results["total_score"] = 0
        return results

    # ── TEST 2: Data Source Quality (15 points) ──────────
    try:
        score = 0

        # Check if JSON-LD was found (5 points)
        # High confidence (>0.9) suggests JSON-LD extraction
        try:
            high_conf_nodes = site.filter(features={1: {"gt": 0.9}}, limit=5)
            if high_conf_nodes and len(high_conf_nodes) >= 3:
                score += 5
                results["diagnostics"]["jsonld_detected"] = True
            elif high_conf_nodes:
                score += 3
                results["diagnostics"]["jsonld_detected"] = True
            else:
                results["diagnostics"]["jsonld_detected"] = False
                results["warnings"].append("DATA: no high-confidence (JSON-LD) nodes found")
        except Exception:
            results["diagnostics"]["jsonld_detected"] = False

        # Check pattern engine contributed (5 points)
        # Medium confidence (0.7-0.9) suggests pattern engine extraction
        try:
            medium_conf = site.filter(features={1: {"gt": 0.7, "lt": 0.95}}, limit=5)
            if medium_conf and len(medium_conf) >= 2:
                score += 5
                results["diagnostics"]["patterns_detected"] = True
            elif medium_conf:
                score += 3
                results["diagnostics"]["patterns_detected"] = True
            else:
                results["diagnostics"]["patterns_detected"] = False
        except Exception:
            results["diagnostics"]["patterns_detected"] = False

        # Check mapping didn't fall back to browser (5 points)
        try:
            low_conf = site.filter(features={1: {"lt": 0.5}}, limit=100)
            total_nodes = site.node_count
            low_conf_count = len(low_conf) if low_conf else 0
            low_conf_ratio = low_conf_count / max(total_nodes, 1)
            if low_conf_ratio < 0.1:
                score += 5
            elif low_conf_ratio < 0.3:
                score += 3
            elif low_conf_ratio < 0.5:
                score += 1
            else:
                results["warnings"].append(f"DATA: {low_conf_ratio:.0%} nodes are low confidence")
            results["diagnostics"]["low_confidence_ratio"] = round(low_conf_ratio, 3)
        except Exception:
            results["diagnostics"]["low_confidence_ratio"] = -1

        results["scores"]["data_source"] = score

    except Exception as e:
        results["scores"]["data_source"] = 0
        results["errors"].append(f"DATA SOURCE CHECK FAILED: {e}")

    # ── TEST 3: Feature Quality (15 points) ──────────────
    try:
        score = 0

        # Non-zero features across multiple dimensions (5 points)
        non_zero_dims = 0
        for dim in [0, 16, 17, 18, 20, 25, 48, 52, 64, 80]:
            try:
                if site.filter(features={dim: {"gt": 0.01}}, limit=1):
                    non_zero_dims += 1
            except Exception:
                pass
        if non_zero_dims >= 7:
            score += 5
        elif non_zero_dims >= 4:
            score += 3
        elif non_zero_dims >= 2:
            score += 1
        results["diagnostics"]["non_zero_dims"] = non_zero_dims

        # Commerce features for e-commerce sites (5 points)
        products = site.filter(page_type=0x04, limit=5)
        if products:
            priced = site.filter(page_type=0x04, features={48: {"gt": 0}}, limit=1)
            rated = site.filter(page_type=0x04, features={52: {"gt": 0}}, limit=1)
            if priced and rated:
                score += 5
            elif priced or rated:
                score += 3
            else:
                score += 1
                results["warnings"].append("FEATURES: products found but no price/rating")
        else:
            articles = site.filter(page_type=0x05, limit=5)
            if articles:
                has_text = site.filter(page_type=0x05, features={17: {"gt": 0.1}}, limit=1)
                if has_text:
                    score += 5
                else:
                    score += 3
            else:
                score += 3  # some features present

        # Trust dimension populated (5 points)
        tls = site.filter(features={80: {"gt": 0.5}}, limit=1)
        if tls:
            score += 5
        else:
            score += 2
            results["warnings"].append("FEATURES: TLS feature not populated")

        results["scores"]["features"] = score

    except Exception as e:
        results["scores"]["features"] = 0
        results["errors"].append(f"FEATURES FAILED: {e}")

    # ── TEST 4: Querying (15 points) ─────────────────────
    try:
        score = 0

        # Basic filter (5 points)
        r = site.filter(limit=10)
        if r and len(r) > 0:
            score += 5
        else:
            results["warnings"].append("QUERY: empty filter result")

        # Feature range filter (5 points)
        r = site.filter(features={17: {"gt": 0.1}}, limit=10)
        if r and len(r) > 0:
            score += 5
        else:
            score += 2

        # Nearest neighbor (5 points)
        goal = [0.0] * 128
        goal[0] = 0.5
        r = site.nearest(goal, k=5)
        if r and len(r) > 0:
            score += 5
        else:
            results["warnings"].append("QUERY: nearest returned empty")

        results["scores"]["query"] = score

    except Exception as e:
        results["scores"]["query"] = 0
        results["errors"].append(f"QUERY FAILED: {e}")

    # ── TEST 5: Pathfinding (10 points) ──────────────────
    try:
        score = 0
        nodes = site.filter(limit=20)

        # Root to deep node (5 points)
        if len(nodes) >= 2:
            path = site.pathfind(from_node=0, to_node=nodes[-1].index)
            if path:
                score += 5
            else:
                score += 2
                results["warnings"].append("PATHFIND: no path to deep node")

        # Cross-graph path (5 points)
        if len(nodes) >= 10:
            path = site.pathfind(from_node=nodes[2].index, to_node=nodes[8].index)
            if path:
                score += 5
            else:
                score += 2
                results["warnings"].append("PATHFIND: no cross-graph path")

        results["scores"]["pathfinding"] = score

    except Exception as e:
        results["scores"]["pathfinding"] = 0
        results["errors"].append(f"PATHFIND FAILED: {e}")

    # ── TEST 6: Action Discovery (15 points) ─────────────
    try:
        score = 0

        # Any actions found on any node (5 points)
        try:
            nodes_with_actions = site.filter(features={96: {"gt": 0.01}}, limit=10)
            action_count = len(nodes_with_actions) if nodes_with_actions else 0
            if action_count >= 3:
                score += 5
            elif action_count > 0:
                score += 3
            else:
                results["warnings"].append("ACTIONS: no actions discovered on any node")
                score += 1
            results["diagnostics"]["nodes_with_actions"] = action_count
        except Exception:
            results["diagnostics"]["nodes_with_actions"] = 0
            score += 1

        # HTTP-executable actions found (5 points)
        try:
            safe_actions = site.filter(features={97: {"gt": 0.5}}, limit=5)
            safe_count = len(safe_actions) if safe_actions else 0
            if safe_count >= 2:
                score += 5
                results["diagnostics"]["http_actions_found"] = True
            elif safe_count > 0:
                score += 3
                results["diagnostics"]["http_actions_found"] = True
            else:
                results["diagnostics"]["http_actions_found"] = False
                results["warnings"].append("ACTIONS: no HTTP-executable actions found")
        except Exception:
            results["diagnostics"]["http_actions_found"] = False

        # Platform detection (5 points)
        try:
            products_with_actions = site.filter(
                page_type=0x04,
                features={96: {"gt": 0.01}},
                limit=3,
            )
            if products_with_actions:
                score += 5
                results["diagnostics"]["platform_actions"] = True
            else:
                score += 2
                results["diagnostics"]["platform_actions"] = False
        except Exception:
            score += 2
            results["diagnostics"]["platform_actions"] = False

        results["scores"]["actions"] = score

    except Exception as e:
        results["scores"]["actions"] = 0
        results["errors"].append(f"ACTIONS FAILED: {e}")

    # ── TEST 7: Live Verification (10 points) ────────────
    try:
        score = 0

        # Perceive homepage (5 points)
        try:
            page = perceive(f"https://{domain}", include_content=True)
            if page:
                score += 3
                if hasattr(page, "features") and page.features:
                    score += 1
                if hasattr(page, "content") and page.content:
                    score += 1
        except Exception as e:
            results["errors"].append(f"PERCEIVE homepage failed: {e}")

        # Perceive interior page (5 points)
        try:
            nodes = site.filter(limit=10)
            if len(nodes) >= 3:
                page = perceive(nodes[2].url)
                if page:
                    score += 5
        except Exception as e:
            results["errors"].append(f"PERCEIVE interior failed: {e}")

        results["scores"]["live"] = score

    except Exception as e:
        results["scores"]["live"] = 0
        results["errors"].append(f"LIVE FAILED: {e}")

    # ── Compute Total ────────────────────────────────────
    results["total_score"] = sum(
        results["scores"].get(k, 0)
        for k in ["mapping", "data_source", "features", "query", "pathfinding", "actions", "live"]
    )

    return results


def run_full_test_suite_v2():
    """Run all 100 site tests and produce v2 report."""
    all_results = []

    print("Cortex v2 — 100-Site Test Suite")
    print("Architecture: Layered Acquisition (no-browser mapping)")
    print(f"{'=' * 60}")
    sys.stdout.flush()

    def _site_timeout_handler(signum, frame):
        raise TimeoutError("Site test timed out")

    for i, domain in enumerate(SITES):
        print(f"\n[{i + 1}/100] {domain}...")
        sys.stdout.flush()

        try:
            # Per-site timeout of 90 seconds
            old_handler = signal.signal(signal.SIGALRM, _site_timeout_handler)
            signal.alarm(90)
            result = test_site_v2(domain)
            signal.alarm(0)
            signal.signal(signal.SIGALRM, old_handler)
            all_results.append(result)

            sc = result["total_score"]
            ds = result["diagnostics"]

            icon = "OK" if sc >= 80 else "!!" if sc >= 50 else "XX"
            jsonld = "J" if ds.get("jsonld_detected") else "-"
            patterns = "P" if ds.get("patterns_detected") else "-"
            actions = "A" if ds.get("http_actions_found") else "-"

            print(
                f"  {icon} {sc:3d}/100  [{jsonld}{patterns}{actions}]  "
                f"nodes:{ds.get('node_count', 0)}  "
                f"time:{ds.get('map_time_s', 0):.1f}s  "
                f"errors:{len(result['errors'])}"
            )

            if sc < 80:
                for err in result["errors"][:3]:
                    print(f"    ERROR: {err}")

        except Exception as e:
            signal.alarm(0)
            print(f"  XX CRASH: {e}")
            traceback.print_exc()
            all_results.append({
                "domain": domain, "total_score": 0,
                "errors": [f"CRASH: {e}"], "warnings": [],
                "scores": {}, "diagnostics": {},
            })

        sys.stdout.flush()
        time.sleep(1)

    # Generate report
    scores = [r["total_score"] for r in all_results]
    avg = sum(scores) / len(scores) if scores else 0

    report = {
        "version": "v2",
        "architecture": "layered_acquisition",
        "summary": {
            "total_sites": len(all_results),
            "average_score": round(avg, 1),
            "sites_above_90": sum(1 for s in scores if s >= 90),
            "sites_above_80": sum(1 for s in scores if s >= 80),
            "sites_above_50": sum(1 for s in scores if s >= 50),
            "sites_below_50": sum(1 for s in scores if s < 50),
            "total_errors": sum(len(r["errors"]) for r in all_results),
            "total_warnings": sum(len(r["warnings"]) for r in all_results),
            "jsonld_coverage": round(
                sum(1 for r in all_results if r.get("diagnostics", {}).get("jsonld_detected"))
                / max(len(all_results), 1), 3
            ),
            "pattern_coverage": round(
                sum(1 for r in all_results if r.get("diagnostics", {}).get("patterns_detected"))
                / max(len(all_results), 1), 3
            ),
            "http_action_coverage": round(
                sum(1 for r in all_results if r.get("diagnostics", {}).get("http_actions_found"))
                / max(len(all_results), 1), 3
            ),
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

    report_path = "/Users/omoshola/Documents/cortex/test-report-v2.json"
    with open(report_path, "w") as f:
        json.dump(report, f, indent=2)

    print(f"\n{'=' * 60}")
    print("RESULTS — v2 Architecture")
    print(f"{'=' * 60}")
    print(f"Average:     {avg:.1f}/100")
    print(f"Sites >=90:  {report['summary']['sites_above_90']}/100")
    print(f"Sites >=80:  {report['summary']['sites_above_80']}/100")
    print(f"Sites <50:   {report['summary']['sites_below_50']}/100")
    print(f"JSON-LD:     {report['summary']['jsonld_coverage']:.0%} of sites")
    print(f"Patterns:    {report['summary']['pattern_coverage']:.0%} of sites")
    print(f"HTTP Acts:   {report['summary']['http_action_coverage']:.0%} of sites")
    print(f"\nCategory scores:")
    for cat, data in report["category_scores"].items():
        print(f"  {cat:15s} avg:{data['average']:5.1f}  min:{data['min']:3d}  max:{data['max']:3d}")
    print(f"\nSaved to {report_path}")

    return report


if __name__ == "__main__":
    run_full_test_suite_v2()
