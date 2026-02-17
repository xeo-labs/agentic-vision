# Copyright 2026 Cortex Contributors
# SPDX-License-Identifier: Apache-2.0
"""Auto-start the Cortex runtime if not running."""

from __future__ import annotations

import os
import shutil
import socket
import subprocess
import time

from .connection import DEFAULT_SOCKET_PATH
from .errors import CortexConnectionError


def ensure_running(
    socket_path: str = DEFAULT_SOCKET_PATH,
    timeout: float = 15.0,
) -> None:
    """Ensure the Cortex runtime is running.

    If the runtime is not reachable, attempt to start it automatically.
    If the binary is not found, raises with a clear install message.

    Args:
        socket_path: Path to the Unix socket.
        timeout: Maximum seconds to wait for startup.

    Raises:
        CortexConnectionError: If the runtime cannot be started.
    """
    if _is_responsive(socket_path):
        return

    # Find the cortex binary
    binary = _find_binary()
    if binary is None:
        raise CortexConnectionError(
            "Could not find 'cortex' binary. "
            "Install Cortex: cargo install cortex-runtime, "
            "or add the cortex binary to your PATH.",
            code="E_BINARY_NOT_FOUND",
        )

    # Start the daemon
    try:
        subprocess.Popen(
            [binary, "start"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
    except OSError as e:
        raise CortexConnectionError(
            f"Failed to start Cortex: {e}. Try starting manually: {binary} start",
            code="E_START_FAILED",
        )

    # Wait for socket to become responsive
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if _is_responsive(socket_path):
            return
        time.sleep(0.5)

    raise CortexConnectionError(
        f"Cortex did not start within {timeout:.0f}s. "
        "Check 'cortex doctor' for diagnostics.",
        code="E_START_TIMEOUT",
    )


def _is_responsive(socket_path: str) -> bool:
    """Check if the socket exists and responds to a handshake."""
    if not os.path.exists(socket_path):
        return False

    try:
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
            sock.settimeout(2.0)
            sock.connect(socket_path)
            import json

            request = (
                json.dumps(
                    {
                        "id": "ping",
                        "method": "handshake",
                        "params": {"client_version": "0.1.0", "protocol_version": 1},
                    }
                ).encode("utf-8")
                + b"\n"
            )
            sock.sendall(request)
            data = sock.recv(4096)
            if data:
                resp = json.loads(data.decode("utf-8").strip())
                return "result" in resp
    except (OSError, ValueError):
        pass
    return False


def _find_binary() -> str | None:
    """Find the cortex binary."""
    candidates = [
        shutil.which("cortex"),
        os.path.expanduser("~/.cortex/bin/cortex"),
        os.path.expanduser("~/.cargo/bin/cortex"),
        "/usr/local/bin/cortex",
    ]
    for path in candidates:
        if path and os.path.isfile(path) and os.access(path, os.X_OK):
            return path
    return None
