#!/usr/bin/env python3
# Copyright 2026 Cortex Contributors
# SPDX-License-Identifier: Apache-2.0
"""LangChain integration: Use Cortex tools in a LangChain agent.

Registers Cortex map/query/act as LangChain tools so an LLM-based
agent can autonomously map websites and search them.

Requirements:
    pip install cortex-agent cortex-langchain langchain langchain-openai

Usage:
    export OPENAI_API_KEY=sk-...
    python 06_langchain_agent.py
"""
from cortex_langchain import CortexMapTool, CortexQueryTool, CortexActTool


def main() -> None:
    # Define Cortex tools for LangChain
    tools = [CortexMapTool(), CortexQueryTool(), CortexActTool()]

    print("Cortex tools registered for LangChain:")
    for tool in tools:
        print(f"  - {tool.name}: {tool.description[:60]}...")

    # -- To use with an actual LLM agent, uncomment below --
    #
    # from langchain_openai import ChatOpenAI
    # from langchain.agents import initialize_agent, AgentType
    #
    # llm = ChatOpenAI(model="gpt-4o", temperature=0)
    # agent = initialize_agent(
    #     tools,
    #     llm,
    #     agent=AgentType.ZERO_SHOT_REACT_DESCRIPTION,
    #     verbose=True,
    # )
    #
    # result = agent.run(
    #     "Map example.com and tell me how many pages it has. "
    #     "Then find any article pages."
    # )
    # print(result)

    # Demo: Use tools directly (no LLM needed)
    print("\n--- Direct tool usage (no LLM) ---")
    map_result = tools[0].run("example.com")
    print(f"Map result: {map_result}")

    query_result = tools[1].run({"domain": "example.com", "limit": 5})
    print(f"Query result: {query_result}")


if __name__ == "__main__":
    main()
