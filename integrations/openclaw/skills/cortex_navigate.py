# Copyright 2026 Cortex Contributors
# SPDX-License-Identifier: Apache-2.0
"""OpenClaw skill: Query and pathfind through a Cortex SiteMap."""
from __future__ import annotations

from typing import Any

import cortex_client


def manifest() -> dict[str, Any]:
    """Return the skill manifest."""
    return {
        "name": "cortex_navigate",
        "description": "Query and pathfind through a previously mapped website.",
        "parameters": {
            "domain": {"type": "string", "required": True},
            "action": {
                "type": "string",
                "required": True,
                "enum": ["query", "pathfind"],
            },
            "page_type": {"type": "integer", "required": False},
            "from_node": {"type": "integer", "required": False},
            "to_node": {"type": "integer", "required": False},
            "limit": {"type": "integer", "required": False, "default": 10},
        },
        "returns": {"type": "object"},
    }


def run(
    domain: str,
    action: str = "query",
    page_type: int | None = None,
    from_node: int = 0,
    to_node: int = 1,
    limit: int = 10,
    **kwargs: Any,
) -> dict[str, Any]:
    """Execute the navigate skill."""
    sm = cortex_client.map(domain, max_render=5)

    if action == "query":
        results = sm.filter(page_type=page_type, limit=limit)
        return {
            "matches": [
                {"index": m.index, "url": m.url, "page_type": m.page_type}
                for m in results
            ]
        }

    if action == "pathfind":
        path = sm.pathfind(from_node, to_node)
        if path is None:
            return {"path": None}
        return {
            "nodes": path.nodes,
            "hops": path.hops,
            "total_weight": path.total_weight,
        }

    return {"error": f"unknown action: {action}"}
