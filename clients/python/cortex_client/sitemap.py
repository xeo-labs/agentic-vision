# Copyright 2026 Cortex Contributors
# SPDX-License-Identifier: Apache-2.0
"""SiteMap class for navigating mapped websites.

Example::

    from cortex_client import map

    site = map("amazon.com")
    products = site.filter(page_type=0x04, features={48: {"lt": 300}})
    path = site.pathfind(from_node=0, to_node=products[0].index)
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Iterator

from .connection import Connection
from .errors import CortexActError, CortexPathError, CortexResourceError
from . import protocol

# Feature vector dimension count.
FEATURE_DIM = 128


@dataclass
class NodeMatch:
    """A matched node from a query.

    Example::

        >>> match = site.filter(page_type=0x04, limit=1)[0]
        >>> match
        NodeMatch(index=4821, url='https://...', type=product_detail, confidence=0.95)
    """

    index: int
    url: str
    page_type: int
    confidence: float
    features: dict[int, float] = field(default_factory=dict)
    similarity: float | None = None

    def __repr__(self) -> str:
        url_short = self.url[:50] + "..." if len(self.url) > 50 else self.url
        type_name = _page_type_name(self.page_type)
        sim = (
            f", similarity={self.similarity:.3f}" if self.similarity is not None else ""
        )
        return (
            f"NodeMatch(index={self.index}, url={url_short!r}, "
            f"type={type_name}, confidence={self.confidence:.2f}{sim})"
        )


@dataclass
class Path:
    """A path through the site graph.

    Example::

        >>> path = site.pathfind(0, 4821)
        >>> path
        Path(hops=3, nodes=[0, 12, 341, 4821], weight=3.0)
    """

    nodes: list[int]
    total_weight: float
    hops: int
    required_actions: list[PathAction]

    def __repr__(self) -> str:
        return (
            f"Path(hops={self.hops}, nodes={self.nodes}, "
            f"weight={self.total_weight:.1f})"
        )


@dataclass
class PathAction:
    """An action required at a specific node along a path."""

    at_node: int
    opcode: tuple[int, int]

    def __repr__(self) -> str:
        return f"PathAction(node={self.at_node}, opcode=({self.opcode[0]:#04x}, {self.opcode[1]:#04x}))"


@dataclass
class RefreshResult:
    """Result of refreshing nodes.

    Example::

        >>> result = site.refresh(nodes=[0, 1, 2])
        >>> result
        RefreshResult(updated=3, changed=[1])
    """

    updated_count: int
    changed_nodes: list[int]

    def __repr__(self) -> str:
        return (
            f"RefreshResult(updated={self.updated_count}, changed={self.changed_nodes})"
        )


@dataclass
class ActResult:
    """Result of executing an action.

    Example::

        >>> result = site.act(node=42, opcode=(0x02, 0x00))
        >>> result
        ActResult(success=True, new_url='https://...')
    """

    success: bool
    new_url: str | None = None
    features: dict[int, float] = field(default_factory=dict)

    def __repr__(self) -> str:
        url = f", new_url={self.new_url!r}" if self.new_url else ""
        return f"ActResult(success={self.success}{url})"


@dataclass
class WatchDelta:
    """A change detected during watching."""

    node: int
    changed_features: dict[int, tuple[float, float]]
    timestamp: float

    def __repr__(self) -> str:
        dims = list(self.changed_features.keys())
        return f"WatchDelta(node={self.node}, changed_dims={dims})"


class SiteMap:
    """Navigable binary site map.

    Wraps protocol responses to provide a convenient query interface.
    All methods send requests to the Cortex daemon via the connection.

    Example::

        >>> site = cortex_client.map("amazon.com")
        >>> site
        SiteMap(domain='amazon.com', nodes=47832, edges=142891)
        >>> site.filter(page_type=0x04, limit=3)
        [NodeMatch(...), NodeMatch(...), NodeMatch(...)]
    """

    def __init__(
        self,
        conn: Connection,
        domain: str,
        node_count: int,
        edge_count: int,
        map_path: str | None = None,
        cached: bool = False,
    ) -> None:
        self._conn = conn
        self.domain = domain
        self.node_count = node_count
        self.edge_count = edge_count
        self.map_path = map_path
        self.cached = cached

    def __repr__(self) -> str:
        cached_str = ", cached=True" if self.cached else ""
        return (
            f"SiteMap(domain={self.domain!r}, nodes={self.node_count}, "
            f"edges={self.edge_count}{cached_str})"
        )

    def filter(
        self,
        *,
        page_type: int | list[int] | None = None,
        features: dict[int, dict[str, float]] | None = None,
        flags: dict[str, bool] | None = None,
        sort_by: tuple[int, str] | None = None,
        limit: int = 100,
    ) -> list[NodeMatch]:
        """Filter nodes by type, features, and flags.

        Args:
            page_type: Filter by page type code(s). E.g. ``0x04`` for product pages.
            features: Feature dimension filters. E.g. ``{48: {"lt": 300}}`` for price < $300.
            flags: Flag filters. E.g. ``{"rendered": True}``.
            sort_by: Sort by a feature dimension. E.g. ``(48, "asc")`` for price ascending.
            limit: Maximum results to return.

        Returns:
            List of matching nodes. Empty list if no matches (never None).

        Example::

            # Find products under $300 with rating > 0.8
            results = site.filter(
                page_type=0x04,
                features={48: {"lt": 300}, 52: {"gt": 0.8}},
                limit=20,
            )
        """
        params = protocol.query_request(
            self.domain,
            page_type=page_type,
            features=features,
            flags=flags,
            sort_by=sort_by,
            limit=limit,
        )
        resp = self._conn.send("query", params)
        return _parse_node_matches(resp)

    def nearest(self, goal_vector: list[float], k: int = 10) -> list[NodeMatch]:
        """Find k nearest nodes by cosine similarity to a goal vector.

        Args:
            goal_vector: A 128-dimension feature vector to compare against.
            k: Number of nearest neighbors to return.

        Returns:
            List of nodes sorted by similarity (highest first).

        Raises:
            ValueError: If goal_vector is not exactly 128 dimensions.

        Example::

            # Find pages similar to a known product page
            goal = [0.0] * 128
            goal[0] = 0x04  # product_detail type
            goal[48] = 250.0  # target price
            similar = site.nearest(goal, k=5)
        """
        if len(goal_vector) != FEATURE_DIM:
            raise ValueError(
                f"Goal vector must be {FEATURE_DIM} dimensions, got {len(goal_vector)}"
            )
        params = protocol.query_request(self.domain, limit=k)
        params["goal_vector"] = goal_vector
        params["mode"] = "nearest"
        resp = self._conn.send("query", params)
        return _parse_node_matches(resp)

    def pathfind(
        self,
        from_node: int,
        to_node: int,
        *,
        avoid_flags: list[str] | None = None,
        minimize: str = "hops",
    ) -> Path | None:
        """Find shortest path between two nodes.

        Args:
            from_node: Source node index.
            to_node: Target node index.
            avoid_flags: Flags to avoid (e.g. ``["auth_required"]``).
            minimize: What to minimize — ``"hops"`` or ``"weight"``.

        Returns:
            A Path object, or None if no path exists.

        Raises:
            CortexPathError: If a node ID is invalid.

        Example::

            path = site.pathfind(0, 4821)
            if path:
                print(f"Found path with {path.hops} hops")
                for node in path.nodes:
                    print(f"  Visit node {node}")
        """
        params = protocol.pathfind_request(
            self.domain,
            from_node,
            to_node,
            avoid_flags=avoid_flags,
            minimize=minimize,
        )
        resp = self._conn.send("pathfind", params)

        if "error" in resp:
            code = resp["error"].get("code", "")
            if code == "E_NO_PATH":
                return None
            raise CortexPathError(
                resp["error"].get("message", "pathfind error"),
                code=code or "E_PATH_FAILED",
            )

        result = resp.get("result", {})
        actions = [
            PathAction(at_node=a["at_node"], opcode=tuple(a["opcode"]))
            for a in result.get("required_actions", [])
        ]
        return Path(
            nodes=result.get("nodes", []),
            total_weight=result.get("total_weight", 0.0),
            hops=result.get("hops", 0),
            required_actions=actions,
        )

    def refresh(
        self,
        *,
        nodes: list[int] | None = None,
        cluster: int | None = None,
        stale_threshold: float | None = None,
    ) -> RefreshResult:
        """Re-render specific nodes and update the map.

        Args:
            nodes: Specific node indices to refresh.
            cluster: Refresh all nodes in a cluster.
            stale_threshold: Only refresh nodes with freshness below this.

        Returns:
            RefreshResult with counts of updated and changed nodes.

        Example::

            result = site.refresh(nodes=[0, 1, 2])
            print(f"{result.updated_count} nodes refreshed")
        """
        params = protocol.refresh_request(
            self.domain,
            nodes=nodes,
            cluster=cluster,
            stale_threshold=stale_threshold,
        )
        resp = self._conn.send("refresh", params)
        result = resp.get("result", {})
        return RefreshResult(
            updated_count=result.get("updated_count", 0),
            changed_nodes=result.get("changed_nodes", []),
        )

    def act(
        self,
        node: int,
        opcode: tuple[int, int],
        params: dict[str, Any] | None = None,
        session_id: str | None = None,
    ) -> ActResult:
        """Execute an action on a live page.

        Args:
            node: The node index to act on.
            opcode: Action opcode as ``(category, action)`` tuple.
            params: Optional action parameters (e.g. form values).
            session_id: Session ID for persistent browser state.

        Returns:
            ActResult with success status and optional new URL/features.

        Raises:
            CortexActError: If the action fails or doesn't exist on the page.

        Example::

            # Click "Add to Cart" on a product page
            result = site.act(node=4821, opcode=(0x02, 0x00))
            if result.success:
                print("Added to cart!")
        """
        req_params = protocol.act_request(
            self.domain, node, opcode, params=params, session_id=session_id
        )
        resp = self._conn.send("act", req_params)

        if "error" in resp:
            raise CortexActError(
                resp["error"].get("message", "action failed"),
                code=resp["error"].get("code", "E_ACT_FAILED"),
            )

        result = resp.get("result", {})
        return ActResult(
            success=result.get("success", False),
            new_url=result.get("new_url"),
            features=result.get("features", {}),
        )

    def watch(
        self,
        *,
        nodes: list[int] | None = None,
        cluster: int | None = None,
        features: list[int] | None = None,
        interval_ms: int = 60000,
    ) -> Iterator[WatchDelta]:
        """Monitor nodes for changes over time.

        Args:
            nodes: Specific node indices to watch.
            cluster: Watch all nodes in a cluster.
            features: Feature dimensions to monitor.
            interval_ms: Check interval in milliseconds.

        Returns:
            Iterator yielding WatchDelta objects as changes are detected.

        Example::

            for delta in site.watch(nodes=[42], interval_ms=5000):
                print(f"Node {delta.node} changed: {delta.changed_features}")
        """
        params = protocol.watch_request(
            self.domain,
            nodes=nodes,
            cluster=cluster,
            features=features,
            interval_ms=interval_ms,
        )
        self._conn.send("watch", params)
        return iter([])


def _parse_node_matches(resp: dict[str, Any]) -> list[NodeMatch]:
    """Parse node matches from a protocol response."""
    if "error" in resp:
        raise CortexResourceError(
            resp["error"].get("message", "query error"),
            code=resp["error"].get("code", "E_NOT_FOUND"),
        )

    result = resp.get("result", {})
    matches = result.get("matches", [])
    return [
        NodeMatch(
            index=m.get("index", 0),
            url=m.get("url", ""),
            page_type=m.get("page_type", 0),
            confidence=m.get("confidence", 0.0),
            features=m.get("features", {}),
            similarity=m.get("similarity"),
        )
        for m in matches
    ]


# Page type display names for repr.
_PAGE_TYPE_NAMES: dict[int, str] = {
    0x00: "unknown",
    0x01: "home",
    0x02: "search_results",
    0x03: "product_listing",
    0x04: "product_detail",
    0x05: "article",
    0x06: "review_list",
    0x07: "media_page",
    0x08: "login",
    0x09: "cart",
    0x0A: "checkout",
    0x0B: "account",
    0x0C: "documentation",
    0x0D: "form_page",
    0x0E: "about_page",
    0x0F: "contact_page",
    0x10: "faq",
    0x11: "pricing_page",
}


def _page_type_name(pt: int) -> str:
    """Convert a page type integer to a human-readable name."""
    return _PAGE_TYPE_NAMES.get(pt, f"type_{pt:#04x}")
