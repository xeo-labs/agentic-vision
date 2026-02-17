# Copyright 2026 Cortex Contributors
# SPDX-License-Identifier: Apache-2.0
"""OpenClaw skill: Map a website with Cortex."""
from __future__ import annotations

from typing import Any

import cortex_client


def manifest() -> dict[str, Any]:
    """Return the skill manifest."""
    return {
        "name": "cortex_map",
        "description": "Map an entire website into a navigable binary graph.",
        "parameters": {
            "domain": {"type": "string", "required": True, "description": "Domain to map"},
            "max_render": {
                "type": "integer",
                "required": False,
                "default": 50,
                "description": "Max pages to render",
            },
        },
        "returns": {
            "type": "object",
            "properties": {
                "domain": {"type": "string"},
                "node_count": {"type": "integer"},
                "edge_count": {"type": "integer"},
            },
        },
    }


def run(domain: str, max_render: int = 50, **kwargs: Any) -> dict[str, Any]:
    """Execute the map skill."""
    sm = cortex_client.map(domain, max_render=max_render)
    return {
        "domain": sm.domain,
        "node_count": sm.node_count,
        "edge_count": sm.edge_count,
    }
