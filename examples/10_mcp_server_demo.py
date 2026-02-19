#!/usr/bin/env python3
# Copyright 2026 Cortex Contributors
# SPDX-License-Identifier: Apache-2.0
"""MCP Server demo: Show how agents connect via Model Context Protocol.

The MCP server (`integrations/mcp-server/`) exposes Cortex tools over
stdio transport, allowing Claude Desktop, Claude Code, Cursor, Windsurf,
and Continue to use Cortex directly. This script demonstrates the
equivalent functionality using the Python client.

Usage:
    pip install cortex-agent
    python 10_mcp_server_demo.py

To actually use the MCP server with an AI agent:
    cortex plug           # Auto-discovers and configures all agents
    cortex plug --list    # See which agents are detected
    cortex plug --status  # Check injection status
"""
import json
import cortex_client


def simulate_mcp_tool_call(tool_name: str, arguments: dict) -> str:
    """Simulate what the MCP server does when an agent calls a tool.

    The real MCP server receives JSON-RPC calls from agents and
    translates them into cortex_client calls. This function mirrors
    that behavior for demonstration purposes.
    """
    if tool_name == "cortex_map":
        site = cortex_client.map(
            arguments["domain"],
            max_render=arguments.get("max_render", 200),
        )
        return json.dumps({
            "domain": site.domain,
            "node_count": site.node_count,
            "edge_count": site.edge_count,
        })

    elif tool_name == "cortex_query":
        site = cortex_client.map(arguments["domain"])
        results = site.filter(
            page_type=arguments.get("page_type"),
            limit=arguments.get("limit", 20),
        )
        return json.dumps([
            {"index": m.index, "url": m.url, "page_type": m.page_type}
            for m in results
        ])

    elif tool_name == "cortex_pathfind":
        site = cortex_client.map(arguments["domain"])
        path = site.pathfind(arguments["from_node"], arguments["to_node"])
        if path is None:
            return json.dumps({"path": None})
        return json.dumps({
            "nodes": path.nodes,
            "hops": path.hops,
            "total_weight": path.total_weight,
        })

    elif tool_name == "cortex_perceive":
        result = cortex_client.perceive(arguments["url"])
        return json.dumps({
            "url": result.final_url,
            "page_type": result.page_type,
            "confidence": result.confidence,
        })

    elif tool_name == "cortex_compare":
        result = cortex_client.compare(arguments["domains"])
        return json.dumps({
            "domains": result.domains,
            "common_types": result.common_types,
        })

    else:
        return json.dumps({"error": f"Unknown tool: {tool_name}"})


def main() -> None:
    print("=== MCP Server Tool Demo ===\n")
    print("Simulating MCP tool calls as an AI agent would make them:\n")

    # 1. Agent maps a site
    print("1. Agent calls cortex_map:")
    result = simulate_mcp_tool_call("cortex_map", {"domain": "example.com"})
    print(f"   Response: {result}\n")

    # 2. Agent queries the map
    print("2. Agent calls cortex_query:")
    result = simulate_mcp_tool_call("cortex_query", {
        "domain": "example.com",
        "limit": 5,
    })
    print(f"   Response: {result}\n")

    # 3. Agent perceives a page
    print("3. Agent calls cortex_perceive:")
    result = simulate_mcp_tool_call("cortex_perceive", {
        "url": "https://example.com",
    })
    print(f"   Response: {result}\n")

    # 4. Agent compares sites
    print("4. Agent calls cortex_compare:")
    result = simulate_mcp_tool_call("cortex_compare", {
        "domains": ["example.com", "iana.org"],
    })
    print(f"   Response: {result}\n")

    print("In production, these calls happen automatically when an agent")
    print("(Claude, Cursor, etc.) invokes Cortex tools via MCP protocol.")
    print("\nSetup: cortex plug  (auto-configures all detected agents)")


if __name__ == "__main__":
    main()
