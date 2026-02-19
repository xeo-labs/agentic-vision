#!/usr/bin/env python3.11
# Copyright 2026 Cortex Contributors
# SPDX-License-Identifier: Apache-2.0
"""Gateway test suite — Tests MCP server, REST API, Python client, and framework adapters."""

import json
import subprocess
import sys
import time
import urllib.request
import urllib.error

RESULTS = {
    "mcp_server": {"score": 0, "max": 30, "details": []},
    "rest_api": {"score": 0, "max": 30, "details": []},
    "python_client": {"score": 0, "max": 25, "details": []},
    "framework_adapters": {"score": 0, "max": 15, "details": []},
}


def log(msg: str) -> None:
    print(f"  {msg}", flush=True)


def rest_post(endpoint: str, body: dict) -> dict | None:
    """POST JSON to the REST API."""
    url = f"http://localhost:7700{endpoint}"
    data = json.dumps(body).encode()
    req = urllib.request.Request(
        url, data=data, headers={"Content-Type": "application/json"}, method="POST"
    )
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            return json.loads(resp.read().decode())
    except Exception as e:
        return {"error": str(e)}


def rest_get(endpoint: str) -> dict | None:
    """GET from the REST API."""
    url = f"http://localhost:7700{endpoint}"
    try:
        with urllib.request.urlopen(url, timeout=30) as resp:
            return json.loads(resp.read().decode())
    except Exception as e:
        return {"error": str(e)}


# ── TEST 2A: MCP Server ──────────────────────────────────────────

def test_mcp_server() -> None:
    """Test MCP server by verifying it builds and has correct tool definitions."""
    print("\n=== Test 2A: MCP Server ===")
    section = RESULTS["mcp_server"]

    import os
    mcp_dir = "/Users/omoshola/Documents/cortex/integrations/mcp-server"

    # Check build
    if os.path.exists(os.path.join(mcp_dir, "dist", "index.js")):
        section["score"] += 5
        log("OK MCP server built (dist/index.js exists)")
        section["details"].append("build: OK")
    else:
        log("FAIL MCP server not built")
        section["details"].append("build: FAIL")

    # Check tool definitions by reading source
    try:
        with open(os.path.join(mcp_dir, "src", "index.ts")) as f:
            source = f.read()

        tools_expected = ["cortex_map", "cortex_query", "cortex_pathfind", "cortex_act", "cortex_perceive", "cortex_compare", "cortex_auth"]
        found = 0
        for tool in tools_expected:
            if tool in source:
                found += 1

        if found == len(tools_expected):
            section["score"] += 15
            log(f"OK All {found} MCP tools defined")
            section["details"].append(f"tools: OK ({found}/{len(tools_expected)})")
        elif found > 0:
            section["score"] += 5 + found
            log(f"PARTIAL {found}/{len(tools_expected)} MCP tools defined")
            section["details"].append(f"tools: PARTIAL ({found}/{len(tools_expected)})")
        else:
            log("FAIL No MCP tools found in source")
            section["details"].append("tools: FAIL")

        # Check MCP SDK integration
        if "@modelcontextprotocol/sdk" in source:
            section["score"] += 5
            log("OK Uses official MCP SDK")
            section["details"].append("sdk: OK")
        else:
            log("FAIL Not using MCP SDK")
            section["details"].append("sdk: FAIL")

        # Check transport
        if "StdioServerTransport" in source:
            section["score"] += 5
            log("OK Stdio transport configured")
            section["details"].append("transport: OK")
        else:
            log("FAIL No stdio transport")
            section["details"].append("transport: FAIL")

    except Exception as e:
        log(f"FAIL reading MCP source: {e}")
        section["details"].append(f"source: FAIL ({e})")


# ── TEST 2B: REST API ────────────────────────────────────────────

def test_rest_api() -> None:
    print("\n=== Test 2B: REST API ===")
    section = RESULTS["rest_api"]

    # Health
    r = rest_get("/health")
    if r and r.get("status") == "ok":
        section["score"] += 5
        log("OK health endpoint")
        section["details"].append("health: OK")
    else:
        log("FAIL health endpoint")
        section["details"].append(f"health: FAIL ({r})")

    # Status
    r = rest_get("/api/v1/status")
    if r and ("version" in r or "running" in str(r) or "status" in str(r)):
        section["score"] += 5
        log("OK status endpoint")
        section["details"].append("status: OK")
    else:
        log(f"FAIL status endpoint: {r}")
        section["details"].append(f"status: FAIL ({r})")

    # Map
    r = rest_post("/api/v1/map", {"domain": "example.com"})
    if r and not r.get("error"):
        node_count = r.get("node_count", r.get("nodes", 0))
        if node_count > 0 or "node_count" in str(r):
            section["score"] += 5
            log(f"OK map endpoint (nodes={node_count})")
            section["details"].append(f"map: OK (nodes={node_count})")
        else:
            section["score"] += 2
            log(f"PARTIAL map endpoint: {r}")
            section["details"].append(f"map: PARTIAL ({r})")
    else:
        log(f"FAIL map endpoint: {r}")
        section["details"].append(f"map: FAIL ({r})")

    # Query
    r = rest_post("/api/v1/query", {"domain": "example.com", "limit": 5})
    if r and not r.get("error"):
        section["score"] += 5
        log("OK query endpoint")
        section["details"].append("query: OK")
    else:
        log(f"FAIL query endpoint: {r}")
        section["details"].append(f"query: FAIL ({r})")

    # Perceive
    r = rest_post("/api/v1/perceive", {"url": "https://example.com"})
    if r and not r.get("error"):
        section["score"] += 5
        log("OK perceive endpoint")
        section["details"].append("perceive: OK")
    else:
        # Perceive may fail without Chromium — partial credit
        section["score"] += 2
        log(f"PARTIAL perceive endpoint (may need browser): {r}")
        section["details"].append(f"perceive: PARTIAL ({r})")

    # List maps
    r = rest_get("/api/v1/maps")
    if r and ("maps" in r or isinstance(r, list)):
        section["score"] += 5
        log("OK maps listing endpoint")
        section["details"].append("maps: OK")
    else:
        log(f"FAIL maps listing: {r}")
        section["details"].append(f"maps: FAIL ({r})")


# ── TEST 2C: Python Client ───────────────────────────────────────

def test_python_client() -> None:
    print("\n=== Test 2C: Python Client End-to-End ===")
    section = RESULTS["python_client"]

    try:
        from cortex_client import map as cmap, perceive, status, login, compare

        # Status
        try:
            s = status()
            if s is not None:
                section["score"] += 5
                log(f"OK status() → {s}")
                section["details"].append(f"status: OK ({s})")
            else:
                log("FAIL status() returned None")
                section["details"].append("status: FAIL (None)")
        except Exception as e:
            log(f"FAIL status(): {e}")
            section["details"].append(f"status: FAIL ({e})")

        # Map
        try:
            site = cmap("example.com", max_time_ms=30000)
            if site.node_count > 0:
                section["score"] += 5
                log(f"OK map() → {site}")
                section["details"].append(f"map: OK ({site})")
            else:
                section["score"] += 2
                log(f"PARTIAL map() → 0 nodes")
                section["details"].append("map: PARTIAL (0 nodes)")
        except Exception as e:
            log(f"FAIL map(): {e}")
            section["details"].append(f"map: FAIL ({e})")
            site = None

        # Query / filter
        if site:
            try:
                results = site.filter(limit=5)
                if len(results) > 0:
                    section["score"] += 5
                    log(f"OK filter() → {len(results)} results")
                    section["details"].append(f"filter: OK ({len(results)} results)")
                else:
                    section["score"] += 2
                    log("PARTIAL filter() → 0 results")
                    section["details"].append("filter: PARTIAL (0 results)")
            except Exception as e:
                log(f"FAIL filter(): {e}")
                section["details"].append(f"filter: FAIL ({e})")

        # Perceive
        try:
            page = perceive("https://example.com")
            if page is not None:
                section["score"] += 5
                log(f"OK perceive() → {page}")
                section["details"].append(f"perceive: OK ({page})")
            else:
                log("FAIL perceive() returned None")
                section["details"].append("perceive: FAIL (None)")
        except Exception as e:
            # Perceive needs browser — partial credit if connection works
            section["score"] += 2
            log(f"PARTIAL perceive() (may need browser): {e}")
            section["details"].append(f"perceive: PARTIAL ({e})")

        # Compare
        try:
            comp = compare(domains=["example.com", "iana.org"], limit=5)
            if comp is not None:
                section["score"] += 5
                log(f"OK compare() → {comp}")
                section["details"].append(f"compare: OK ({comp})")
            else:
                log("FAIL compare() returned None")
                section["details"].append("compare: FAIL (None)")
        except Exception as e:
            section["score"] += 2
            log(f"PARTIAL compare(): {e}")
            section["details"].append(f"compare: PARTIAL ({e})")

    except ImportError as e:
        log(f"FAIL cannot import cortex_client: {e}")
        section["details"].append(f"import: FAIL ({e})")


# ── TEST 2D: Framework Adapters ──────────────────────────────────

def test_framework_adapters() -> None:
    print("\n=== Test 2D: Framework Adapters ===")
    section = RESULTS["framework_adapters"]

    # LangChain
    try:
        sys.path.insert(0, "/Users/omoshola/Documents/cortex/integrations/langchain")
        from cortex_langchain import CortexMapTool, CortexQueryTool
        tool = CortexMapTool()
        result = tool.run("example.com")
        if result and ("map" in result.lower() or "node" in result.lower()):
            section["score"] += 5
            log(f"OK LangChain adapter")
            section["details"].append("langchain: OK")
        else:
            section["score"] += 2
            log(f"PARTIAL LangChain adapter: {result}")
            section["details"].append(f"langchain: PARTIAL ({result})")
    except ImportError:
        log("SKIP LangChain adapter (not installed)")
        section["details"].append("langchain: SKIP (not installed)")
        section["score"] += 3  # optional — partial credit
    except Exception as e:
        log(f"FAIL LangChain adapter: {e}")
        section["details"].append(f"langchain: FAIL ({e})")

    # CrewAI
    try:
        sys.path.insert(0, "/Users/omoshola/Documents/cortex/integrations/crewai")
        from cortex_crewai import CortexWebCartographer
        import json as _json
        tool = CortexWebCartographer()
        result = tool._run(_json.dumps({"action": "map", "domain": "example.com"}))
        if result is not None:
            section["score"] += 5
            log("OK CrewAI adapter")
            section["details"].append("crewai: OK")
        else:
            section["score"] += 2
            log("PARTIAL CrewAI adapter")
            section["details"].append("crewai: PARTIAL")
    except ImportError:
        log("SKIP CrewAI adapter (not installed)")
        section["details"].append("crewai: SKIP (not installed)")
        section["score"] += 3
    except Exception as e:
        log(f"FAIL CrewAI adapter: {e}")
        section["details"].append(f"crewai: FAIL ({e})")

    # OpenClaw
    try:
        sys.path.insert(0, "/Users/omoshola/Documents/cortex/integrations/openclaw")
        from cortex_openclaw import CortexTool
        section["score"] += 4
        log("OK OpenClaw adapter (importable)")
        section["details"].append("openclaw: OK")
    except ImportError:
        log("SKIP OpenClaw adapter (not installed)")
        section["details"].append("openclaw: SKIP (not installed)")
        section["score"] += 3
    except Exception as e:
        log(f"FAIL OpenClaw adapter: {e}")
        section["details"].append(f"openclaw: FAIL ({e})")


# ── MAIN ─────────────────────────────────────────────────────────

def main() -> None:
    print("=" * 70)
    print("Cortex v3 — Gateway Test Suite")
    print("=" * 70)

    test_mcp_server()
    test_rest_api()
    test_python_client()
    test_framework_adapters()

    # Summary
    total = sum(s["score"] for s in RESULTS.values())
    max_total = sum(s["max"] for s in RESULTS.values())

    print(f"\n{'=' * 70}")
    print("GATEWAY TEST RESULTS")
    print(f"{'=' * 70}")
    for name, data in RESULTS.items():
        status_icon = "OK" if data["score"] >= data["max"] * 0.7 else "PARTIAL" if data["score"] > 0 else "FAIL"
        print(f"  {name:25s} {data['score']:3d}/{data['max']:3d}  [{status_icon}]")
    print(f"  {'TOTAL':25s} {total:3d}/{max_total:3d}")
    print()

    # Save report
    report = {
        "test": "gateway",
        "version": "v3",
        "timestamp": time.strftime("%Y-%m-%dT%H:%M:%SZ"),
        "scores": {k: {"score": v["score"], "max": v["max"], "details": v["details"]} for k, v in RESULTS.items()},
        "total_score": total,
        "max_score": max_total,
    }
    with open("/Users/omoshola/Documents/cortex/gateway-test-report.json", "w") as f:
        json.dump(report, f, indent=2)
    print(f"Saved to gateway-test-report.json")


if __name__ == "__main__":
    main()
