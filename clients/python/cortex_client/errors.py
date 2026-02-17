# Copyright 2026 Cortex Contributors
# SPDX-License-Identifier: Apache-2.0
"""Exception types for the Cortex client.

Every error includes a ``code`` attribute for programmatic handling and a
human-readable message answering: what happened, why, and how to fix it.
"""


class CortexError(Exception):
    """Base exception for all Cortex errors.

    Attributes:
        code: Machine-readable error code (e.g. ``"E_CONNECTION"``).
        message: Human-readable description.

    Example::

        try:
            site = cortex_client.map("example.com")
        except CortexError as e:
            print(f"Error [{e.code}]: {e.message}")
    """

    def __init__(self, message: str, *, code: str = "E_UNKNOWN") -> None:
        self.code = code
        self.message = message
        super().__init__(message)

    def __repr__(self) -> str:
        return (
            f"{self.__class__.__name__}(code={self.code!r}, message={self.message!r})"
        )


class CortexConnectionError(CortexError):
    """Cannot connect to the Cortex runtime.

    Common causes: daemon not running, socket path wrong, permission denied.
    Fix: run ``cortex start`` or check that the socket path is correct.
    """

    def __init__(self, message: str, *, code: str = "E_CONNECTION") -> None:
        super().__init__(message, code=code)


class CortexTimeoutError(CortexError):
    """Operation timed out.

    Increase the ``timeout_ms`` parameter or check if the site is very slow.
    """

    def __init__(self, message: str, *, code: str = "E_TIMEOUT") -> None:
        super().__init__(message, code=code)


class CortexResourceError(CortexError):
    """A requested resource (map, node, page) was not found."""

    def __init__(self, message: str, *, code: str = "E_NOT_FOUND") -> None:
        super().__init__(message, code=code)


class CortexMapError(CortexError):
    """Error during mapping operation.

    The site may be unreachable, block automated access, or require auth.
    """

    def __init__(self, message: str, *, code: str = "E_MAP_FAILED") -> None:
        super().__init__(message, code=code)


class CortexPathError(CortexError):
    """Error during pathfinding operation.

    No path may exist between the given nodes, or a node ID is invalid.
    """

    def __init__(self, message: str, *, code: str = "E_PATH_FAILED") -> None:
        super().__init__(message, code=code)


class CortexActError(CortexError):
    """Error executing an action on a page.

    The action may not exist on this page, or the page has changed since mapping.
    """

    def __init__(self, message: str, *, code: str = "E_ACT_FAILED") -> None:
        super().__init__(message, code=code)


class CortexSetupError(CortexError):
    """Cortex environment is not correctly set up.

    Chromium may not be installed or the runtime binary may be missing.
    """

    def __init__(self, message: str, *, code: str = "E_SETUP") -> None:
        super().__init__(message, code=code)
