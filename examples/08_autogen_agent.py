#!/usr/bin/env python3
# Copyright 2026 Cortex Contributors
# SPDX-License-Identifier: Apache-2.0
"""AutoGen integration: Register Cortex functions with AutoGen agents.

Demonstrates registering cortex_map, cortex_query, and cortex_act
as function-calling tools for an AutoGen AssistantAgent.

Requirements:
    pip install cortex-agent cortex-autogen pyautogen

Usage:
    export OPENAI_API_KEY=sk-...
    python 08_autogen_agent.py
"""
from cortex_autogen import cortex_map, cortex_query, cortex_act


def main() -> None:
    # Show available functions
    functions = [cortex_map, cortex_query, cortex_act]
    print("Cortex functions registered for AutoGen:")
    for fn in functions:
        print(f"  - {fn.__name__}: {fn.__doc__.strip().splitlines()[0]}")

    # -- To use with AutoGen agents, uncomment below --
    #
    # from autogen import AssistantAgent, UserProxyAgent
    #
    # llm_config = {
    #     "model": "gpt-4o",
    #     "temperature": 0,
    # }
    #
    # assistant = AssistantAgent(
    #     "web_analyst",
    #     llm_config=llm_config,
    #     system_message=(
    #         "You are a web analyst. Use cortex_map to map websites, "
    #         "cortex_query to search them, and cortex_act to interact."
    #     ),
    # )
    #
    # user = UserProxyAgent(
    #     "user",
    #     human_input_mode="NEVER",
    #     code_execution_config=False,
    #     max_consecutive_auto_reply=5,
    # )
    #
    # # Register Cortex functions
    # for fn in [cortex_map, cortex_query, cortex_act]:
    #     assistant.register_for_llm(name=fn.__name__)(fn)
    #     user.register_for_execution(name=fn.__name__)(fn)
    #
    # user.initiate_chat(
    #     assistant,
    #     message="Map example.com and tell me what pages it has.",
    # )

    # Demo: Call functions directly
    print("\n--- Direct function calls (no LLM) ---")
    result = cortex_map("example.com")
    print(f"Map: {result}")

    result = cortex_query("example.com", limit=5)
    print(f"Query: {result}")


if __name__ == "__main__":
    main()
