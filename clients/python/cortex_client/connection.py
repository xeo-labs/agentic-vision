# Copyright 2026 Cortex Contributors
# SPDX-License-Identifier: Apache-2.0
"""Unix socket connection to the Cortex runtime."""

from __future__ import annotations

import json
import socket
import time
from typing import Any

from .errors import CortexConnectionError, CortexTimeoutError

DEFAULT_SOCKET_PATH = "/tmp/cortex.sock"
DEFAULT_TIMEOUT = 60.0


class Connection:
    """Low-level connection to the Cortex runtime via Unix domain socket.

    Supports context manager protocol for clean resource management::

        with Connection() as conn:
            result = conn.send("status")
    """

    def __init__(
        self,
        socket_path: str = DEFAULT_SOCKET_PATH,
        timeout: float = DEFAULT_TIMEOUT,
    ) -> None:
        self._socket_path = socket_path
        self._timeout = timeout
        self._sock: socket.socket | None = None
        self._buffer = b""

    def connect(self) -> None:
        """Connect to the Cortex runtime socket.

        Raises:
            CortexConnectionError: If connection fails.
        """
        try:
            self._sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            self._sock.settimeout(self._timeout)
            self._sock.connect(self._socket_path)
        except FileNotFoundError:
            raise CortexConnectionError(
                f"Cannot connect to Cortex at {self._socket_path}. "
                "The process may not be running. Start it with: cortex start",
                code="E_SOCKET_NOT_FOUND",
            )
        except PermissionError:
            raise CortexConnectionError(
                f"Permission denied on {self._socket_path}. "
                "Check file permissions or run 'cortex stop && cortex start'.",
                code="E_PERMISSION_DENIED",
            )
        except ConnectionRefusedError:
            raise CortexConnectionError(
                f"Cortex refused connection at {self._socket_path}. "
                "The process may have crashed. Try 'cortex stop && cortex start'.",
                code="E_CONNECTION_REFUSED",
            )
        except OSError as e:
            raise CortexConnectionError(
                f"Cannot connect to Cortex: {e}",
                code="E_CONNECTION",
            )

    def close(self) -> None:
        """Close the connection."""
        if self._sock:
            try:
                self._sock.close()
            except OSError:
                pass
            self._sock = None
        self._buffer = b""

    def send(self, method: str, params: dict[str, Any] | None = None) -> dict[str, Any]:
        """Send a request and return the response.

        Auto-connects if not already connected. Retries once on broken pipe.

        Args:
            method: Protocol method name (e.g. ``"map"``, ``"query"``).
            params: Method parameters.

        Returns:
            The response dict with ``"result"`` or ``"error"`` key.

        Raises:
            CortexConnectionError: If not connected or connection broken.
            CortexTimeoutError: If the operation times out.
        """
        if self._sock is None:
            self.connect()

        request = {
            "id": f"req-{time.monotonic_ns()}",
            "method": method,
            "params": params or {},
        }

        request_bytes = json.dumps(request).encode("utf-8") + b"\n"

        try:
            assert self._sock is not None
            self._sock.sendall(request_bytes)
        except BrokenPipeError:
            self.close()
            self.connect()
            assert self._sock is not None
            self._sock.sendall(request_bytes)
        except socket.timeout:
            raise CortexTimeoutError(
                f"Timeout sending {method} request after {self._timeout:.0f}s. "
                "The Cortex daemon may be overloaded. "
                "Try increasing the timeout or check 'cortex status'.",
                code="E_SEND_TIMEOUT",
            )

        return self._read_response()

    def _read_response(self) -> dict[str, Any]:
        """Read a newline-delimited JSON response."""
        assert self._sock is not None
        while b"\n" not in self._buffer:
            try:
                chunk = self._sock.recv(65536)
            except socket.timeout:
                raise CortexTimeoutError(
                    "Timeout waiting for response from Cortex. "
                    "The operation may be taking longer than expected. "
                    "Try increasing the timeout parameter.",
                    code="E_RECV_TIMEOUT",
                )
            if not chunk:
                raise CortexConnectionError(
                    "Connection closed by Cortex daemon. "
                    "The process may have crashed. "
                    "Check 'cortex doctor' for diagnostics.",
                    code="E_CONNECTION_CLOSED",
                )
            self._buffer += chunk

        line, self._buffer = self._buffer.split(b"\n", 1)
        result: dict[str, Any] = json.loads(line.decode("utf-8"))
        return result

    @property
    def is_connected(self) -> bool:
        """Whether this connection is currently open."""
        return self._sock is not None

    def __repr__(self) -> str:
        status = "connected" if self.is_connected else "disconnected"
        return f"Connection(path={self._socket_path!r}, {status})"

    def __enter__(self) -> Connection:
        self.connect()
        return self

    def __exit__(self, *args: object) -> None:
        self.close()
