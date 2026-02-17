# Copyright 2026 Cortex Contributors
# SPDX-License-Identifier: Apache-2.0
"""Cortex Client — Thin client for the Cortex web cartography runtime.

Quick start::

    import cortex_client

    site = cortex_client.map("amazon.com")
    products = site.filter(page_type=0x04, limit=5)
    path = site.pathfind(0, products[0].index)
"""

from __future__ import annotations

from dataclasses import dataclass, field
from urllib.parse import urlparse

from .autostart import ensure_running
from .connection import Connection, DEFAULT_SOCKET_PATH
from .errors import (
    CortexActError,
    CortexConnectionError,
    CortexError,
    CortexMapError,
    CortexPathError,
    CortexResourceError,
    CortexSetupError,
    CortexTimeoutError,
)
from .sitemap import (
    ActResult,
    NodeMatch,
    Path,
    PathAction,
    RefreshResult,
    SiteMap,
    WatchDelta,
)
from . import protocol

__version__ = "0.1.0"

__all__ = [
    # Top-level functions
    "map",
    "map_many",
    "perceive",
    "perceive_many",
    "status",
    # Utilities
    "normalize_domain",
    # Classes
    "SiteMap",
    "NodeMatch",
    "Path",
    "PathAction",
    "RefreshResult",
    "ActResult",
    "WatchDelta",
    "RuntimeStatus",
    "PageResult",
    "Connection",
    # Errors
    "CortexError",
    "CortexConnectionError",
    "CortexTimeoutError",
    "CortexResourceError",
    "CortexMapError",
    "CortexPathError",
    "CortexActError",
    "CortexSetupError",
]


def normalize_domain(domain: str) -> str:
    """Normalize a domain input by stripping protocol, path, and trailing slashes.

    Handles common user mistakes:
    - ``"https://amazon.com/dp/B0EXAMPLE"`` → ``"amazon.com"``
    - ``"http://example.com/"`` → ``"example.com"``
    - ``"example.com/"`` → ``"example.com"``
    - ``"localhost:3000"`` → ``"localhost:3000"`` (ports preserved)

    Args:
        domain: Domain string, possibly with protocol/path/trailing slash.

    Returns:
        Cleaned domain string.

    Raises:
        ValueError: If domain is empty after normalization.
    """
    if not domain or not domain.strip():
        raise ValueError("domain cannot be empty")

    d = domain.strip()

    # If it looks like a URL (has ://), parse it
    if "://" in d:
        parsed = urlparse(d)
        d = parsed.netloc or parsed.path.split("/")[0]
    elif d.startswith("//"):
        d = d[2:].split("/")[0]
    else:
        # Strip any path component
        d = d.split("/")[0]

    # Strip trailing dots
    d = d.rstrip(".")

    if not d:
        raise ValueError(f"domain cannot be empty (input was {domain!r})")

    return d


@dataclass
class RuntimeStatus:
    """Status of the Cortex runtime.

    Example::

        >>> s = cortex_client.status()
        >>> s
        RuntimeStatus(version='0.1.0', uptime=3600.0s, maps=3)
    """

    version: str
    uptime_seconds: float
    active_contexts: int
    cached_maps: int
    memory_mb: float

    def __repr__(self) -> str:
        return (
            f"RuntimeStatus(version={self.version!r}, "
            f"uptime={self.uptime_seconds:.1f}s, "
            f"maps={self.cached_maps})"
        )


@dataclass
class PageResult:
    """Result of perceiving a single page.

    Example::

        >>> page = cortex_client.perceive("https://example.com")
        >>> page
        PageResult(url='https://example.com', type=home, confidence=0.92)
    """

    url: str
    final_url: str
    page_type: int
    confidence: float
    features: dict[int, float] = field(default_factory=dict)
    content: str | None = None

    def __repr__(self) -> str:
        from .sitemap import _page_type_name

        type_name = _page_type_name(self.page_type)
        return (
            f"PageResult(url={self.url!r}, type={type_name}, "
            f"confidence={self.confidence:.2f})"
        )


def map(
    domain: str,
    *,
    max_nodes: int = 50000,
    max_render: int = 200,
    max_time_ms: int = 10000,
    respect_robots: bool = True,
    socket_path: str = DEFAULT_SOCKET_PATH,
    timeout_ms: int = 30000,
) -> SiteMap:
    """Map a website and return a navigable SiteMap.

    Args:
        domain: The domain to map. Accepts full URLs — protocol and path
            will be stripped automatically. E.g. ``"amazon.com"`` or
            ``"https://amazon.com/dp/B0EXAMPLE"``.
        max_nodes: Maximum number of nodes to include.
        max_render: Maximum pages to render with a browser.
        max_time_ms: Maximum mapping time in milliseconds.
        respect_robots: Whether to respect robots.txt.
        socket_path: Path to the Cortex Unix socket.
        timeout_ms: Client timeout in milliseconds.

    Returns:
        A SiteMap object for querying and navigating.

    Raises:
        ValueError: If domain is empty.
        CortexConnectionError: If Cortex is not running and cannot be started.
        CortexMapError: If the mapping operation fails.
        CortexTimeoutError: If mapping exceeds the timeout.

    Example::

        site = cortex_client.map("amazon.com")
        print(f"{site.node_count} nodes, {site.edge_count} edges")
    """
    domain = normalize_domain(domain)
    ensure_running(socket_path)
    conn = Connection(socket_path, timeout=timeout_ms / 1000.0)
    params = protocol.map_request(
        domain,
        max_nodes=max_nodes,
        max_render=max_render,
        max_time_ms=max_time_ms,
        respect_robots=respect_robots,
    )
    resp = conn.send("map", params)
    if "error" in resp:
        err = resp["error"]
        raise CortexMapError(
            err.get("message", "map failed"),
            code=err.get("code", "E_MAP_FAILED"),
        )
    result = resp.get("result", {})
    return SiteMap(
        conn=conn,
        domain=domain,
        node_count=result.get("node_count", 0),
        edge_count=result.get("edge_count", 0),
        map_path=result.get("map_path"),
    )


def map_many(
    domains: list[str],
    *,
    max_nodes: int = 50000,
    max_render: int = 200,
    max_time_ms: int = 10000,
    respect_robots: bool = True,
    socket_path: str = DEFAULT_SOCKET_PATH,
) -> list[SiteMap]:
    """Map multiple websites.

    Args:
        domains: List of domains to map. Each is normalized automatically.
        max_nodes: Maximum nodes per domain.
        max_render: Maximum pages to render per domain.
        max_time_ms: Maximum mapping time per domain.
        respect_robots: Whether to respect robots.txt.
        socket_path: Path to the Cortex Unix socket.

    Returns:
        List of SiteMap objects.

    Example::

        sites = cortex_client.map_many(["amazon.com", "bestbuy.com"])
    """
    return [
        map(
            d,
            max_nodes=max_nodes,
            max_render=max_render,
            max_time_ms=max_time_ms,
            respect_robots=respect_robots,
            socket_path=socket_path,
        )
        for d in domains
    ]


def perceive(
    url: str,
    *,
    include_content: bool = True,
    socket_path: str = DEFAULT_SOCKET_PATH,
) -> PageResult:
    """Perceive a single page and return its encoding.

    Args:
        url: The URL to perceive (full URL with protocol).
        include_content: Whether to include raw text content.
        socket_path: Path to the Cortex Unix socket.

    Returns:
        A PageResult with the page's feature encoding and optional content.

    Example::

        page = cortex_client.perceive("https://amazon.com/dp/B0EXAMPLE")
        print(f"Page type: {page.page_type}, confidence: {page.confidence}")
    """
    ensure_running(socket_path)
    conn = Connection(socket_path)
    params = protocol.perceive_request(url, include_content=include_content)
    resp = conn.send("perceive", params)
    if "error" in resp:
        err = resp["error"]
        raise CortexResourceError(
            err.get("message", "perceive failed"),
            code=err.get("code", "E_NOT_FOUND"),
        )
    result = resp.get("result", {})
    return PageResult(
        url=url,
        final_url=result.get("final_url", url),
        page_type=result.get("page_type", 0),
        confidence=result.get("confidence", 0.0),
        features=result.get("features", {}),
        content=result.get("content"),
    )


def perceive_many(
    urls: list[str],
    *,
    include_content: bool = True,
    socket_path: str = DEFAULT_SOCKET_PATH,
) -> list[PageResult]:
    """Perceive multiple pages.

    Args:
        urls: List of URLs to perceive.
        include_content: Whether to include raw text content.
        socket_path: Path to the Cortex Unix socket.

    Returns:
        List of PageResult objects.
    """
    return [
        perceive(u, include_content=include_content, socket_path=socket_path)
        for u in urls
    ]


def status(
    *,
    socket_path: str = DEFAULT_SOCKET_PATH,
) -> RuntimeStatus:
    """Get Cortex runtime status.

    Args:
        socket_path: Path to the Cortex Unix socket.

    Returns:
        RuntimeStatus with version, uptime, and resource info.

    Raises:
        CortexConnectionError: If Cortex is not running.

    Example::

        s = cortex_client.status()
        print(f"Cortex v{s.version}, {s.cached_maps} maps cached")
    """
    ensure_running(socket_path)
    conn = Connection(socket_path)
    resp = conn.send("status")
    if "error" in resp:
        err = resp["error"]
        raise CortexConnectionError(
            err.get("message", "status failed"),
            code=err.get("code", "E_CONNECTION"),
        )
    result = resp.get("result", {})
    return RuntimeStatus(
        version=result.get("version", "unknown"),
        uptime_seconds=result.get("uptime_seconds", 0.0),
        active_contexts=result.get("active_contexts", 0),
        cached_maps=result.get("cached_maps", 0),
        memory_mb=result.get("memory_mb", 0.0),
    )
