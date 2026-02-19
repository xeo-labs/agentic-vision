#!/usr/bin/env python3
# Copyright 2026 Cortex Contributors
# SPDX-License-Identifier: Apache-2.0
"""CrewAI integration: Build a web research crew powered by Cortex.

Creates a CrewAI agent with the CortexWebCartographer tool, enabling
autonomous web mapping and research without manual browsing.

Requirements:
    pip install cortex-agent cortex-crewai crewai

Usage:
    export OPENAI_API_KEY=sk-...
    python 07_crewai_researcher.py
"""
from cortex_crewai import CortexWebCartographer


def main() -> None:
    # Create the Cortex tool for CrewAI
    cartographer = CortexWebCartographer()

    print(f"CrewAI tool registered: {cartographer.name}")
    print(f"  Description: {cartographer.description[:80]}...")

    # -- To run with a full CrewAI crew, uncomment below --
    #
    # from crewai import Agent, Task, Crew
    #
    # researcher = Agent(
    #     role="Web Research Analyst",
    #     goal="Map websites and extract structured intelligence",
    #     backstory=(
    #         "You are a web analyst who uses Cortex to map entire "
    #         "websites in seconds and find specific pages by type."
    #     ),
    #     tools=[cartographer],
    #     verbose=True,
    # )
    #
    # task = Task(
    #     description=(
    #         "Map example.com and iana.org. Compare their structures. "
    #         "Report how many pages each site has and what types of "
    #         "content they contain."
    #     ),
    #     expected_output="A structured comparison of both sites.",
    #     agent=researcher,
    # )
    #
    # crew = Crew(agents=[researcher], tasks=[task], verbose=True)
    # result = crew.kickoff()
    # print(result)

    # Demo: Use the tool directly
    print("\n--- Direct tool usage (no LLM) ---")
    result = cartographer._run(domain="example.com")
    print(f"Cartographer result: {result}")


if __name__ == "__main__":
    main()
