#!/usr/bin/env python3
# Copyright 2026 Cortex Contributors
# SPDX-License-Identifier: Apache-2.0
"""Semantic Kernel integration: Use Cortex as a kernel plugin.

Registers the CortexPlugin with a Semantic Kernel instance, exposing
cortex_map, cortex_query, cortex_pathfind, and cortex_act as kernel
functions that an LLM planner can invoke.

Requirements:
    pip install cortex-agent cortex-semantic-kernel semantic-kernel

Usage:
    export OPENAI_API_KEY=sk-...
    python 09_semantic_kernel.py
"""
from cortex_semantic_kernel import CortexPlugin


def main() -> None:
    # Create the plugin
    plugin = CortexPlugin()

    # Show available kernel functions
    print("Cortex Semantic Kernel Plugin:")
    print(f"  - map_site: {plugin.map_site.__doc__}")
    print(f"  - query_site: {plugin.query_site.__doc__}")
    print(f"  - pathfind: {plugin.pathfind.__doc__}")
    print(f"  - act: {plugin.act.__doc__}")

    # -- To use with a full Semantic Kernel setup, uncomment below --
    #
    # from semantic_kernel import Kernel
    # from semantic_kernel.connectors.ai.open_ai import OpenAIChatCompletion
    #
    # kernel = Kernel()
    # kernel.add_plugin(CortexPlugin(), plugin_name="cortex")
    # kernel.add_service(OpenAIChatCompletion(
    #     service_id="chat",
    #     ai_model_id="gpt-4o",
    # ))
    #
    # # Invoke a function directly
    # result = await kernel.invoke(
    #     plugin_name="cortex",
    #     function_name="cortex_map",
    #     domain="example.com",
    # )
    # print(result)

    # Demo: Call plugin methods directly
    print("\n--- Direct plugin usage (no LLM) ---")
    result = plugin.map_site(domain="example.com")
    print(f"Map: {result}")

    result = plugin.query_site(domain="example.com", limit=5)
    print(f"Query: {result}")


if __name__ == "__main__":
    main()
