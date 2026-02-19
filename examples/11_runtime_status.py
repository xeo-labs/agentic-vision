#!/usr/bin/env python3
# Copyright 2026 Cortex Contributors
# SPDX-License-Identifier: Apache-2.0
"""Runtime status and health monitoring.

Check the Cortex daemon status, view cached maps, and monitor uptime.
Useful for building dashboards or health checks in production.

Usage:
    pip install cortex-agent
    python 11_runtime_status.py
"""
import cortex_client


def main() -> None:
    # Get runtime status
    status = cortex_client.status()

    print("=== Cortex Runtime Status ===\n")
    print(f"  Version:         {status.version}")
    print(f"  Uptime:          {status.uptime_seconds:.0f}s")
    print(f"  Active contexts: {status.active_contexts}")
    print(f"  Cached maps:     {status.cached_maps}")
    print(f"  Memory usage:    {status.memory_mb:.1f} MB")

    if status.cached_maps > 0:
        print("\n  Cached domains:")
        # Map a known domain to show it in the cache
        site = cortex_client.map("example.com")
        print(f"    - {site.domain}: {site.node_count} nodes")


if __name__ == "__main__":
    main()
