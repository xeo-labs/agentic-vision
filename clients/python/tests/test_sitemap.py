# Copyright 2026 Cortex Contributors
# SPDX-License-Identifier: Apache-2.0
"""Unit tests for SiteMap dataclasses and protocol helpers."""

from __future__ import annotations

from unittest.mock import MagicMock

import pytest

from cortex_client import (
    NodeMatch,
    Path,
    PathAction,
    RefreshResult,
    ActResult,
    WatchDelta,
    PageResult,
    RuntimeStatus,
)
from cortex_client.sitemap import SiteMap, _parse_node_matches
from cortex_client.protocol import (
    map_request,
    query_request,
    pathfind_request,
    refresh_request,
    act_request,
    perceive_request,
    watch_request,
)


# ---------------------------------------------------------------------------
# Dataclass construction
# ---------------------------------------------------------------------------


class TestNodeMatch:
    def test_basic_construction(self) -> None:
        m = NodeMatch(index=1, url="https://example.com", page_type=3, confidence=0.95)
        assert m.index == 1
        assert m.url == "https://example.com"
        assert m.page_type == 3
        assert m.confidence == 0.95
        assert m.features == {}
        assert m.similarity is None

    def test_with_features_and_similarity(self) -> None:
        m = NodeMatch(
            index=5,
            url="https://example.com/p",
            page_type=7,
            confidence=0.8,
            features={0: 1.0, 48: 29.99},
            similarity=0.92,
        )
        assert m.features[48] == pytest.approx(29.99)
        assert m.similarity == pytest.approx(0.92)


class TestPath:
    def test_basic_path(self) -> None:
        action = PathAction(at_node=2, opcode=(4, 0))
        p = Path(
            nodes=[0, 1, 2, 3], total_weight=3.5, hops=3, required_actions=[action]
        )
        assert len(p.nodes) == 4
        assert p.hops == 3
        assert p.required_actions[0].opcode == (4, 0)


class TestRefreshResult:
    def test_construction(self) -> None:
        r = RefreshResult(updated_count=5, changed_nodes=[1, 3, 7])
        assert r.updated_count == 5
        assert len(r.changed_nodes) == 3


class TestActResult:
    def test_success(self) -> None:
        a = ActResult(
            success=True, new_url="https://example.com/cart", features={48: 0.0}
        )
        assert a.success is True
        assert a.new_url == "https://example.com/cart"

    def test_failure(self) -> None:
        a = ActResult(success=False)
        assert a.success is False
        assert a.new_url is None


class TestWatchDelta:
    def test_construction(self) -> None:
        d = WatchDelta(
            node=10,
            changed_features={48: (29.99, 24.99)},
            timestamp=1700000000.0,
        )
        assert d.node == 10
        old, new = d.changed_features[48]
        assert old == pytest.approx(29.99)
        assert new == pytest.approx(24.99)


class TestPageResult:
    def test_basic(self) -> None:
        pr = PageResult(
            url="https://example.com",
            final_url="https://example.com/",
            page_type=1,
            confidence=0.9,
        )
        assert pr.content is None
        assert pr.features == {}


class TestRuntimeStatus:
    def test_basic(self) -> None:
        s = RuntimeStatus(
            version="0.1.0",
            uptime_seconds=120.5,
            active_contexts=3,
            cached_maps=2,
            memory_mb=512.0,
        )
        assert s.version == "0.1.0"
        assert s.active_contexts == 3


# ---------------------------------------------------------------------------
# _parse_node_matches helper
# ---------------------------------------------------------------------------


class TestParseNodeMatches:
    def test_success(self) -> None:
        resp = {
            "result": {
                "matches": [
                    {
                        "index": 0,
                        "url": "https://a.com",
                        "page_type": 1,
                        "confidence": 0.9,
                    },
                    {
                        "index": 5,
                        "url": "https://b.com",
                        "page_type": 7,
                        "confidence": 0.8,
                        "features": {48: 19.99},
                        "similarity": 0.75,
                    },
                ]
            }
        }
        matches = _parse_node_matches(resp)
        assert len(matches) == 2
        assert matches[0].url == "https://a.com"
        assert matches[1].similarity == pytest.approx(0.75)

    def test_empty(self) -> None:
        resp = {"result": {"matches": []}}
        assert _parse_node_matches(resp) == []

    def test_error_raises(self) -> None:
        from cortex_client.errors import CortexResourceError

        resp = {"error": {"message": "bad query"}}
        with pytest.raises(CortexResourceError, match="bad query"):
            _parse_node_matches(resp)


# ---------------------------------------------------------------------------
# Protocol message builders
# ---------------------------------------------------------------------------


class TestProtocol:
    def test_map_request(self) -> None:
        req = map_request("example.com", max_nodes=100)
        assert req["domain"] == "example.com"
        assert req["max_nodes"] == 100
        assert req["respect_robots"] is True

    def test_query_request_minimal(self) -> None:
        req = query_request("example.com")
        assert req["domain"] == "example.com"
        assert req["limit"] == 100
        assert "page_type" not in req

    def test_query_request_with_page_type(self) -> None:
        req = query_request("example.com", page_type=7)
        assert req["page_type"] == [7]

    def test_query_request_with_page_type_list(self) -> None:
        req = query_request("example.com", page_type=[1, 2, 7])
        assert req["page_type"] == [1, 2, 7]

    def test_query_request_with_sort(self) -> None:
        req = query_request("example.com", sort_by=(48, "desc"))
        assert req["sort_by"] == {"dimension": 48, "direction": "desc"}

    def test_pathfind_request(self) -> None:
        req = pathfind_request("example.com", 0, 10, avoid_flags=["auth_required"])
        assert req["from"] == 0
        assert req["to"] == 10
        assert req["avoid_flags"] == ["auth_required"]
        assert req["minimize"] == "hops"

    def test_refresh_request(self) -> None:
        req = refresh_request("example.com", nodes=[1, 2, 3], stale_threshold=3600.0)
        assert req["nodes"] == [1, 2, 3]
        assert req["stale_threshold"] == 3600.0

    def test_act_request(self) -> None:
        req = act_request("example.com", 5, (2, 0), params={"quantity": 2})
        assert req["node"] == 5
        assert req["opcode"] == [2, 0]
        assert req["params"]["quantity"] == 2

    def test_perceive_request(self) -> None:
        req = perceive_request("https://example.com/page")
        assert req["url"] == "https://example.com/page"
        assert req["include_content"] is True

    def test_watch_request(self) -> None:
        req = watch_request("example.com", features=[48, 49], interval_ms=30000)
        assert req["features"] == [48, 49]
        assert req["interval_ms"] == 30000


# ---------------------------------------------------------------------------
# SiteMap method signatures (mock connection)
# ---------------------------------------------------------------------------


class TestSiteMapMethods:
    def _make_sitemap(self) -> SiteMap:
        conn = MagicMock()
        return SiteMap(
            conn=conn,
            domain="example.com",
            node_count=100,
            edge_count=250,
        )

    def test_filter_sends_query(self) -> None:
        sm = self._make_sitemap()
        sm._conn.send.return_value = {"result": {"matches": []}}
        result = sm.filter(page_type=7, limit=10)
        assert result == []
        sm._conn.send.assert_called_once()
        call_args = sm._conn.send.call_args
        assert call_args[0][0] == "query"

    def test_nearest_sends_query_with_mode(self) -> None:
        sm = self._make_sitemap()
        sm._conn.send.return_value = {"result": {"matches": []}}
        result = sm.nearest([0.0] * 128, k=5)
        assert result == []
        call_args = sm._conn.send.call_args
        assert call_args[0][0] == "query"
        params = call_args[0][1]
        assert params["mode"] == "nearest"

    def test_pathfind_returns_path(self) -> None:
        sm = self._make_sitemap()
        sm._conn.send.return_value = {
            "result": {
                "nodes": [0, 3, 7],
                "total_weight": 2.5,
                "hops": 2,
                "required_actions": [{"at_node": 3, "opcode": [4, 0]}],
            }
        }
        path = sm.pathfind(0, 7)
        assert path is not None
        assert path.nodes == [0, 3, 7]
        assert path.hops == 2
        assert path.required_actions[0].opcode == (4, 0)

    def test_pathfind_returns_none_on_no_path(self) -> None:
        sm = self._make_sitemap()
        sm._conn.send.return_value = {
            "error": {"code": "E_NO_PATH", "message": "no path"}
        }
        assert sm.pathfind(0, 99) is None

    def test_refresh(self) -> None:
        sm = self._make_sitemap()
        sm._conn.send.return_value = {
            "result": {"updated_count": 3, "changed_nodes": [1, 5, 9]}
        }
        result = sm.refresh(nodes=[1, 5, 9])
        assert result.updated_count == 3
        assert result.changed_nodes == [1, 5, 9]

    def test_act(self) -> None:
        sm = self._make_sitemap()
        sm._conn.send.return_value = {
            "result": {"success": True, "new_url": "https://example.com/cart"}
        }
        result = sm.act(5, (2, 0))
        assert result.success is True
        assert result.new_url == "https://example.com/cart"

    def test_nearest_wrong_dimension(self) -> None:
        sm = self._make_sitemap()
        with pytest.raises(ValueError, match="128 dimensions, got 64"):
            sm.nearest([0.0] * 64, k=5)

    def test_nearest_zero_vector(self) -> None:
        sm = self._make_sitemap()
        sm._conn.send.return_value = {"result": {"matches": []}}
        result = sm.nearest([0.0] * 128, k=5)
        assert result == []

    def test_sitemap_repr(self) -> None:
        sm = self._make_sitemap()
        r = repr(sm)
        assert "example.com" in r
        assert "100" in r
        assert "250" in r


# ---------------------------------------------------------------------------
# Domain normalization tests
# ---------------------------------------------------------------------------


class TestNormalizeDomain:
    def test_plain_domain(self) -> None:
        from cortex_client import normalize_domain

        assert normalize_domain("example.com") == "example.com"

    def test_strip_https(self) -> None:
        from cortex_client import normalize_domain

        assert normalize_domain("https://example.com") == "example.com"

    def test_strip_http(self) -> None:
        from cortex_client import normalize_domain

        assert normalize_domain("http://example.com") == "example.com"

    def test_strip_path(self) -> None:
        from cortex_client import normalize_domain

        assert normalize_domain("https://amazon.com/dp/B0EXAMPLE") == "amazon.com"

    def test_strip_trailing_slash(self) -> None:
        from cortex_client import normalize_domain

        assert normalize_domain("example.com/") == "example.com"

    def test_preserve_port(self) -> None:
        from cortex_client import normalize_domain

        assert normalize_domain("localhost:3000") == "localhost:3000"

    def test_empty_domain_raises(self) -> None:
        from cortex_client import normalize_domain

        with pytest.raises(ValueError, match="cannot be empty"):
            normalize_domain("")

    def test_whitespace_only_raises(self) -> None:
        from cortex_client import normalize_domain

        with pytest.raises(ValueError, match="cannot be empty"):
            normalize_domain("   ")


# ---------------------------------------------------------------------------
# Error .code attribute tests
# ---------------------------------------------------------------------------


class TestErrorCodes:
    def test_cortex_error_has_code(self) -> None:
        from cortex_client.errors import CortexError

        e = CortexError("test", code="E_TEST")
        assert e.code == "E_TEST"
        assert e.message == "test"

    def test_connection_error_code(self) -> None:
        from cortex_client.errors import CortexConnectionError

        e = CortexConnectionError("fail")
        assert e.code == "E_CONNECTION"

    def test_timeout_error_code(self) -> None:
        from cortex_client.errors import CortexTimeoutError

        e = CortexTimeoutError("slow")
        assert e.code == "E_TIMEOUT"

    def test_error_repr(self) -> None:
        from cortex_client.errors import CortexError

        e = CortexError("test msg", code="E_TEST")
        r = repr(e)
        assert "E_TEST" in r
        assert "test msg" in r


# ---------------------------------------------------------------------------
# __repr__ tests
# ---------------------------------------------------------------------------


class TestRepr:
    def test_node_match_repr(self) -> None:
        nm = NodeMatch(
            index=42,
            url="https://example.com/product/123",
            page_type=0x04,
            confidence=0.95,
        )
        r = repr(nm)
        assert "42" in r
        assert "product_detail" in r
        assert "0.95" in r

    def test_path_repr(self) -> None:
        p = Path(
            nodes=[0, 3, 7],
            total_weight=2.5,
            hops=2,
            required_actions=[],
        )
        r = repr(p)
        assert "hops=2" in r
        assert "2.5" in r

    def test_runtime_status_repr(self) -> None:
        s = RuntimeStatus(
            version="0.1.0",
            uptime_seconds=3600.0,
            active_contexts=2,
            cached_maps=3,
            memory_mb=340.0,
        )
        r = repr(s)
        assert "0.1.0" in r
        assert "3600" in r

    def test_page_result_repr(self) -> None:
        pr = PageResult(
            url="https://example.com",
            final_url="https://example.com",
            page_type=0x01,
            confidence=0.92,
        )
        r = repr(pr)
        assert "home" in r
        assert "0.92" in r
