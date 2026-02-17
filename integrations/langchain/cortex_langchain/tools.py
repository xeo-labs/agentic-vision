# Copyright 2026 Cortex Contributors
# SPDX-License-Identifier: Apache-2.0
"""LangChain tool wrappers for Cortex."""
from __future__ import annotations

from typing import Any, Optional

try:
    from langchain_core.tools import BaseTool
except ImportError:
    from langchain.tools import BaseTool  # type: ignore[no-redef]

import cortex_client


class CortexMapTool(BaseTool):
    """Map an entire website into a navigable graph."""

    name: str = "cortex_map"
    description: str = (
        "Map an entire website into a navigable graph structure. "
        "Input: domain name (e.g. 'example.com'). "
        "Returns: summary of mapped site with node count and page types."
    )

    def _run(self, domain: str, **kwargs: Any) -> str:
        sm = cortex_client.map(domain, max_render=kwargs.get("max_render", 50))
        return (
            f"Mapped {sm.domain}: {sm.node_count} nodes, {sm.edge_count} edges. "
            f"Use cortex_query to search this map."
        )

    async def _arun(self, domain: str, **kwargs: Any) -> str:
        return self._run(domain, **kwargs)


class CortexQueryTool(BaseTool):
    """Search a mapped website for pages matching criteria."""

    name: str = "cortex_query"
    description: str = (
        "Search a previously mapped website for pages matching criteria. "
        "Input: JSON with 'domain' (required), optional 'page_type' (int), "
        "'limit' (int). Returns: list of matching pages with URLs and types."
    )

    def _run(self, query: str, **kwargs: Any) -> str:
        import json

        params = json.loads(query) if isinstance(query, str) else query
        domain = params.get("domain", "")
        sm = cortex_client.map(domain, max_render=5)
        results = sm.filter(
            page_type=params.get("page_type"),
            limit=params.get("limit", 10),
        )
        lines = [f"Found {len(results)} result(s):"]
        for m in results:
            lines.append(f"  [{m.index}] type={m.page_type} url={m.url}")
        return "\n".join(lines)

    async def _arun(self, query: str, **kwargs: Any) -> str:
        return self._run(query, **kwargs)


class CortexActTool(BaseTool):
    """Execute an action on a live webpage."""

    name: str = "cortex_act"
    description: str = (
        "Execute an action on a live webpage (click button, fill form, etc). "
        "Input: JSON with 'domain', 'node' (int), 'opcode' [category, action]. "
        "Returns: result of the action."
    )

    def _run(self, action: str, **kwargs: Any) -> str:
        import json

        params = json.loads(action) if isinstance(action, str) else action
        domain = params.get("domain", "")
        sm = cortex_client.map(domain, max_render=5)
        result = sm.act(
            params.get("node", 0),
            tuple(params.get("opcode", [1, 0])),
        )
        return f"Action {'succeeded' if result.success else 'failed'}. New URL: {result.new_url}"

    async def _arun(self, action: str, **kwargs: Any) -> str:
        return self._run(action, **kwargs)
