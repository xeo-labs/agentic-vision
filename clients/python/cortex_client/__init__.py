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
from .session import Session
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

__version__ = "1.0.0"

__all__ = [
    # Top-level functions
    "map",
    "map_many",
    "perceive",
    "perceive_many",
    "act",
    "compare",
    "status",
    "login",
    "login_oauth",
    "login_api_key",
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
    "Session",
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
    session: Session | None = None,
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
        session: Optional authenticated session for mapping private content.
            Obtain via :func:`login`, :func:`login_oauth`, or
            :func:`login_api_key`.
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

        # Authenticated mapping:
        session = cortex_client.login("example.com", username="me", password="pw")
        site = cortex_client.map("example.com", session=session)
    """
    domain = normalize_domain(domain)
    ensure_running(socket_path)
    conn = Connection(socket_path, timeout=(timeout_ms / 1000.0) + 15.0)
    params = protocol.map_request(
        domain,
        max_nodes=max_nodes,
        max_render=max_render,
        max_time_ms=max_time_ms,
        respect_robots=respect_robots,
    )
    if session is not None:
        params["session_id"] = session.session_id
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


def login(
    domain: str,
    *,
    username: str,
    password: str,
    socket_path: str = DEFAULT_SOCKET_PATH,
) -> Session:
    """Authenticate with a website via HTTP password login.

    Discovers the login form, fills in credentials, POSTs the form, and
    captures session cookies. No browser needed for standard login forms.

    Args:
        domain: The domain to authenticate with.
        username: Username or email address.
        password: Password.
        socket_path: Path to the Cortex Unix socket.

    Returns:
        A Session for authenticated mapping and actions.

    Raises:
        CortexError: If authentication fails.

    Example::

        session = cortex_client.login("example.com", username="me", password="pw")
        site = cortex_client.map("example.com", session=session)
    """
    domain = normalize_domain(domain)
    ensure_running(socket_path)
    conn = Connection(socket_path)
    resp = conn.send(
        "auth",
        {
            "domain": domain,
            "auth_type": "password",
            "username": username,
            "password": password,
        },
    )
    if "error" in resp:
        err = resp["error"]
        raise CortexError(
            err.get("message", "auth failed"),
            code=err.get("code", "E_AUTH_FAILED"),
        )
    result = resp.get("result", {})
    return Session(
        session_id=result["session_id"],
        domain=result.get("domain", domain),
        auth_type=result.get("auth_type", "password"),
        expires_at=result.get("expires_at"),
    )


def login_oauth(
    domain: str,
    *,
    provider: str = "google",
    socket_path: str = DEFAULT_SOCKET_PATH,
) -> Session:
    """Authenticate via OAuth. Opens browser briefly for consent.

    Args:
        domain: The domain to authenticate with.
        provider: OAuth provider name (e.g. ``"google"``, ``"github"``).
        socket_path: Path to the Cortex Unix socket.

    Returns:
        A Session for authenticated mapping and actions.

    Raises:
        CortexError: If authentication fails.

    Example::

        session = cortex_client.login_oauth("example.com", provider="google")
        site = cortex_client.map("example.com", session=session)
    """
    domain = normalize_domain(domain)
    ensure_running(socket_path)
    conn = Connection(socket_path)
    resp = conn.send(
        "auth",
        {
            "domain": domain,
            "auth_type": "oauth",
            "provider": provider,
        },
    )
    if "error" in resp:
        err = resp["error"]
        raise CortexError(
            err.get("message", "auth failed"),
            code=err.get("code", "E_AUTH_FAILED"),
        )
    result = resp.get("result", {})
    return Session(
        session_id=result["session_id"],
        domain=result.get("domain", domain),
        auth_type=result.get("auth_type", "oauth"),
        expires_at=result.get("expires_at"),
    )


def login_api_key(
    domain: str,
    *,
    key: str,
    header_name: str = "X-Api-Key",
    socket_path: str = DEFAULT_SOCKET_PATH,
) -> Session:
    """Create an API-key authenticated session. No network call needed.

    Args:
        domain: The domain to authenticate with.
        key: The API key value.
        header_name: HTTP header name for the key (default ``X-Api-Key``).
        socket_path: Path to the Cortex Unix socket.

    Returns:
        A Session for authenticated mapping and actions.

    Raises:
        CortexError: If session creation fails.

    Example::

        session = cortex_client.login_api_key("api.example.com", key="sk-...")
        site = cortex_client.map("api.example.com", session=session)
    """
    domain = normalize_domain(domain)
    ensure_running(socket_path)
    conn = Connection(socket_path)
    resp = conn.send(
        "auth",
        {
            "domain": domain,
            "auth_type": "api_key",
            "key": key,
            "header_name": header_name,
        },
    )
    if "error" in resp:
        err = resp["error"]
        raise CortexError(
            err.get("message", "auth failed"),
            code=err.get("code", "E_AUTH_FAILED"),
        )
    result = resp.get("result", {})
    return Session(
        session_id=result["session_id"],
        domain=result.get("domain", domain),
        auth_type=result.get("auth_type", "api_key"),
        expires_at=result.get("expires_at"),
    )


def act(
    url: str,
    opcode: tuple[int, int],
    *,
    params: dict[str, object] | None = None,
    session: Session | None = None,
    socket_path: str = DEFAULT_SOCKET_PATH,
) -> ActResult:
    """Execute an action on a live page.

    Args:
        url: The URL of the page to act on.
        opcode: Action opcode as ``(category, action)`` tuple.
        params: Optional action parameters (e.g. form values).
        session: Optional authenticated session.
        socket_path: Path to the Cortex Unix socket.

    Returns:
        An ActResult with success status and optional new URL/features.

    Raises:
        CortexActError: If the action fails.

    Example::

        result = cortex_client.act("https://amazon.com/dp/B0EXAMPLE", opcode=(0x02, 0x00))
    """
    ensure_running(socket_path)
    conn = Connection(socket_path)
    domain = normalize_domain(url)
    req_params = protocol.act_request(
        domain, 0, opcode, params=params,
        session_id=session.session_id if session else None,
    )
    req_params["url"] = url
    resp = conn.send("act", req_params)
    if "error" in resp:
        raise CortexActError(
            resp["error"].get("message", "action failed"),
            code=resp["error"].get("code", "E_ACT_FAILED"),
        )
    r = resp.get("result", {})
    return ActResult(
        success=r.get("success", False),
        new_url=r.get("new_url"),
        features=r.get("features", {}),
    )


@dataclass
class CompareResult:
    """Result of comparing multiple site maps."""

    domains: list[str]
    common_page_types: list[int]
    unique_per_domain: dict[str, list[str]]
    similarity_matrix: dict[str, dict[str, float]]

    def __repr__(self) -> str:
        return f"CompareResult(domains={self.domains}, common_types={len(self.common_page_types)})"


def compare(
    *,
    domains: list[str],
    limit: int = 100,
    socket_path: str = DEFAULT_SOCKET_PATH,
) -> CompareResult:
    """Map and compare multiple websites.

    Args:
        domains: List of domains to map and compare.
        limit: Maximum nodes per domain for comparison.
        socket_path: Path to the Cortex Unix socket.

    Returns:
        A CompareResult with comparison data.

    Example::

        comp = cortex_client.compare(domains=["amazon.com", "bestbuy.com"], limit=10)
    """
    sites = map_many(domains, socket_path=socket_path)
    common_types: set[int] = set()
    type_sets: list[set[int]] = []
    for s in sites:
        nodes = s.filter(limit=limit)
        ts = {n.page_type for n in nodes}
        type_sets.append(ts)
    if type_sets:
        common_types = type_sets[0]
        for ts in type_sets[1:]:
            common_types &= ts

    unique: dict[str, list[str]] = {}
    for i, s in enumerate(sites):
        others = set()
        for j, ts in enumerate(type_sets):
            if j != i:
                others |= ts
        unique[s.domain] = [str(t) for t in type_sets[i] - others]

    sim: dict[str, dict[str, float]] = {}
    for i, si in enumerate(sites):
        sim[si.domain] = {}
        for j, sj in enumerate(sites):
            if i == j:
                sim[si.domain][sj.domain] = 1.0
            else:
                overlap = len(type_sets[i] & type_sets[j])
                union = len(type_sets[i] | type_sets[j]) or 1
                sim[si.domain][sj.domain] = round(overlap / union, 3)

    return CompareResult(
        domains=[s.domain for s in sites],
        common_page_types=sorted(common_types),
        unique_per_domain=unique,
        similarity_matrix=sim,
    )
