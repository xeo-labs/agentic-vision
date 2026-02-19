# Cortex Examples

Runnable examples demonstrating Cortex capabilities, from basic mapping to multi-agent framework integration.

## Prerequisites

```bash
# Install the Cortex runtime
cargo install cortex-runtime

# Install the Python client
pip install cortex-agent
```

Cortex auto-starts the daemon and handles Chromium installation on first run. No manual setup needed.

## Examples

| # | File | Description | Dependencies |
|---|------|-------------|--------------|
| 01 | `01_quickstart.py` | Map a site and list pages in 10 lines | `cortex-agent` |
| 02 | `02_price_comparison.py` | Compare prices across 5 e-commerce sites | `cortex-agent` |
| 03 | `03_pathfinding.py` | Find shortest navigation route through a site | `cortex-agent` |
| 04 | `04_cross_site_comparison.py` | Compare structures of multiple websites | `cortex-agent` |
| 05 | `05_perceive_page.py` | Analyze a single page without full site mapping | `cortex-agent` |
| 06 | `06_langchain_agent.py` | Use Cortex tools in a LangChain agent | `cortex-langchain`, `langchain` |
| 07 | `07_crewai_researcher.py` | Build a CrewAI research crew with Cortex | `cortex-crewai`, `crewai` |
| 08 | `08_autogen_agent.py` | Register Cortex functions with AutoGen agents | `cortex-autogen`, `pyautogen` |
| 09 | `09_semantic_kernel.py` | Use Cortex as a Semantic Kernel plugin | `cortex-semantic-kernel`, `semantic-kernel` |
| 10 | `10_mcp_server_demo.py` | Simulate MCP tool calls (Claude, Cursor, etc.) | `cortex-agent` |
| 11 | `11_runtime_status.py` | Monitor runtime health and cached maps | `cortex-agent` |

## Running

```bash
# Run any example
python examples/01_quickstart.py

# Framework examples work in two modes:
# 1. Direct mode (no LLM needed) — runs out of the box
# 2. Agent mode (uncomment LLM sections) — requires API keys
```

## Framework Integration Quick Reference

### LangChain
```python
from cortex_langchain import CortexMapTool, CortexQueryTool
tools = [CortexMapTool(), CortexQueryTool()]
agent = initialize_agent(tools, llm, agent="zero-shot-react-description")
```

### CrewAI
```python
from cortex_crewai import CortexWebCartographer
agent = Agent(role="Researcher", tools=[CortexWebCartographer()])
```

### AutoGen
```python
from cortex_autogen import cortex_map, cortex_query
assistant.register_for_llm(name="cortex_map")(cortex_map)
```

### Semantic Kernel
```python
from cortex_semantic_kernel import CortexPlugin
kernel.add_plugin(CortexPlugin(), plugin_name="cortex")
```

### MCP (Claude, Cursor, Windsurf, Continue)
```bash
cortex plug  # One command — auto-discovers and configures all agents
```

## License

Apache-2.0
