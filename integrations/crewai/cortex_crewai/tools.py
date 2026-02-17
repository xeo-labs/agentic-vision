# Copyright 2026 Cortex Contributors
# SPDX-License-Identifier: Apache-2.0
"""CrewAI tool for Cortex web cartography."""
from __future__ import annotations

from typing import Any

try:
    from crewai_tools import BaseTool
except ImportError:

    class BaseTool:  # type: ignore[no-redef]
        name: str = ""
        description: str = ""

        def _run(self, *args: Any, **kwargs: Any) -> Any:
            raise NotImplementedError


import cortex_client


class CortexWebCartographer(BaseTool):
    """Map and query websites as navigable graphs for AI agents."""

    name: str = "cortex_web_cartographer"
    description: str = (
        "Map an entire website into a navigable graph, then query it. "
        "Input: JSON with 'action' ('map', 'query', 'pathfind'), 'domain', "
        "and optional parameters. Returns structured results."
    )

    def _run(self, input_str: str) -> str:
        import json

        params = json.loads(input_str) if isinstance(input_str, str) else input_str
        action = params.get("action", "map")
        domain = params.get("domain", "")

        if action == "map":
            sm = cortex_client.map(domain, max_render=params.get("max_render", 50))
            return json.dumps(
                {
                    "domain": sm.domain,
                    "node_count": sm.node_count,
                    "edge_count": sm.edge_count,
                }
            )

        if action == "query":
            sm = cortex_client.map(domain, max_render=5)
            results = sm.filter(
                page_type=params.get("page_type"),
                limit=params.get("limit", 10),
            )
            return json.dumps(
                [{"index": m.index, "url": m.url, "page_type": m.page_type} for m in results]
            )

        if action == "pathfind":
            sm = cortex_client.map(domain, max_render=5)
            path = sm.pathfind(params.get("from_node", 0), params.get("to_node", 1))
            if path is None:
                return json.dumps({"path": None})
            return json.dumps(
                {"nodes": path.nodes, "hops": path.hops, "weight": path.total_weight}
            )

        return json.dumps({"error": f"unknown action: {action}"})
