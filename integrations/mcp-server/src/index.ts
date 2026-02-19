#!/usr/bin/env node
// Copyright 2026 Cortex Contributors
// SPDX-License-Identifier: Apache-2.0

/**
 * Cortex MCP Server — Model Context Protocol bridge for the Cortex runtime.
 *
 * Translates MCP tool calls into Cortex protocol messages over Unix socket.
 * Used by Claude Desktop, Claude Code, Cursor, Continue, Cline, and any
 * MCP-compatible agent.
 *
 * Usage:
 *   npx @cortex/mcp-server          # stdio transport (default)
 *   cortex-mcp-server               # same, via bin entry
 */

import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";
import { CortexClient } from "./cortex-client.js";

const SOCKET_PATH = process.env["CORTEX_SOCKET"] ?? "/tmp/cortex.sock";

const TOOLS = [
  {
    name: "cortex_map",
    description:
      "Map an entire website into a navigable binary graph. Returns site structure with page types, features (prices, ratings, etc.), and navigation edges. No browser needed — reads structured data via HTTP.",
    inputSchema: {
      type: "object" as const,
      properties: {
        domain: {
          type: "string",
          description: "Domain to map (e.g. 'amazon.com')",
        },
        max_nodes: {
          type: "integer",
          description: "Maximum number of nodes in the map",
          default: 50000,
        },
        max_time_ms: {
          type: "integer",
          description: "Maximum mapping time in milliseconds",
          default: 30000,
        },
      },
      required: ["domain"],
    },
  },
  {
    name: "cortex_query",
    description:
      "Search a mapped site by page type, feature values, or text similarity. Returns matching pages with their features.",
    inputSchema: {
      type: "object" as const,
      properties: {
        domain: {
          type: "string",
          description: "Domain to query (must be previously mapped)",
        },
        page_type: {
          type: "integer",
          description:
            "Filter by page type: 1=home, 2=product_listing, 3=search_results, 4=product_detail, 5=cart, 6=article, 7=documentation, 8=login, 9=checkout, 10=profile, 11=api_endpoint, 12=media, 13=form, 14=dashboard, 15=error, 16=other",
        },
        price_lt: { type: "number", description: "Max price filter" },
        price_gt: { type: "number", description: "Min price filter" },
        rating_gt: {
          type: "number",
          description: "Minimum rating (0.0-1.0)",
        },
        limit: {
          type: "integer",
          description: "Maximum results to return",
          default: 20,
        },
      },
      required: ["domain"],
    },
  },
  {
    name: "cortex_pathfind",
    description:
      "Find the shortest navigation path between two pages on a mapped site.",
    inputSchema: {
      type: "object" as const,
      properties: {
        domain: { type: "string", description: "Domain to pathfind in" },
        from_node: { type: "integer", description: "Source node index" },
        to_node: { type: "integer", description: "Target node index" },
      },
      required: ["domain", "from_node", "to_node"],
    },
  },
  {
    name: "cortex_act",
    description:
      "Execute an action on a website — add to cart, submit form, search, etc. Prefers HTTP execution (no browser). Returns updated page state.",
    inputSchema: {
      type: "object" as const,
      properties: {
        domain: { type: "string", description: "Domain to act on" },
        node: { type: "integer", description: "Target node index" },
        action: {
          type: "string",
          description: "Action to execute",
          enum: [
            "add_to_cart",
            "search",
            "submit_form",
            "login",
            "next_page",
            "apply_filter",
          ],
        },
        params: {
          type: "object",
          description: "Action-specific parameters",
        },
      },
      required: ["domain", "node", "action"],
    },
  },
  {
    name: "cortex_perceive",
    description:
      "Get the current live state of a single page. Returns structured data, features, and available actions.",
    inputSchema: {
      type: "object" as const,
      properties: {
        url: { type: "string", description: "URL to perceive" },
        include_content: {
          type: "boolean",
          description: "Include raw text content",
          default: false,
        },
      },
      required: ["url"],
    },
  },
  {
    name: "cortex_compare",
    description:
      "Compare products, articles, or pages across multiple mapped sites. Maps all sites if not cached, then queries all maps.",
    inputSchema: {
      type: "object" as const,
      properties: {
        domains: {
          type: "array",
          items: { type: "string" },
          description: "Domains to compare across",
        },
        page_type: { type: "string", description: "Page type to compare" },
        sort_by: {
          type: "string",
          description: "Sort order for results",
          enum: ["price_asc", "price_desc", "rating_desc", "review_count_desc"],
        },
        filters: {
          type: "object",
          description: "Feature filters (e.g. price_lt, rating_gt)",
        },
        limit: {
          type: "integer",
          description: "Maximum results per site",
          default: 10,
        },
      },
      required: ["domains"],
    },
  },
  {
    name: "cortex_auth",
    description:
      "Authenticate with a website to access protected content. Supports password, OAuth, and API key authentication.",
    inputSchema: {
      type: "object" as const,
      properties: {
        domain: {
          type: "string",
          description: "Domain to authenticate with",
        },
        method: {
          type: "string",
          description: "Authentication method",
          enum: ["password", "oauth", "api_key"],
        },
        credentials: {
          type: "object",
          description:
            "Auth credentials (e.g. {username, password} or {key, header_name})",
        },
      },
      required: ["domain", "method"],
    },
  },
];

/** Map MCP tool arguments to Cortex protocol params. */
function buildParams(
  toolName: string,
  args: Record<string, unknown>,
): { method: string; params: Record<string, unknown> } {
  switch (toolName) {
    case "cortex_map":
      return {
        method: "map",
        params: {
          domain: args["domain"],
          max_nodes: args["max_nodes"] ?? 50000,
          max_time_ms: args["max_time_ms"] ?? 30000,
        },
      };
    case "cortex_query": {
      const params: Record<string, unknown> = {
        domain: args["domain"],
        limit: args["limit"] ?? 20,
      };
      if (args["page_type"] != null)
        params["page_type"] = args["page_type"];
      // Build feature ranges from shorthand params
      const features: Record<string, Record<string, unknown>> = {};
      if (args["price_lt"] != null)
        features["48"] = { lt: args["price_lt"] };
      if (args["price_gt"] != null)
        features["48"] = { ...features["48"], gt: args["price_gt"] };
      if (args["rating_gt"] != null)
        features["52"] = { gt: args["rating_gt"] };
      if (Object.keys(features).length > 0) params["features"] = features;
      return { method: "query", params };
    }
    case "cortex_pathfind":
      return {
        method: "pathfind",
        params: {
          domain: args["domain"],
          from: args["from_node"],
          to: args["to_node"],
        },
      };
    case "cortex_act":
      return {
        method: "act",
        params: {
          domain: args["domain"],
          node: args["node"],
          opcode: args["action"],
          params: args["params"] ?? {},
        },
      };
    case "cortex_perceive":
      return {
        method: "perceive",
        params: {
          url: args["url"],
          include_content: args["include_content"] ?? false,
        },
      };
    case "cortex_compare": {
      // Compare = map each domain + query across all
      // For now, send as a multi-step operation via map + query
      return {
        method: "map",
        params: {
          domain: (args["domains"] as string[])?.[0] ?? "",
          max_time_ms: 30000,
          _compare: true,
          _all_domains: args["domains"],
          _page_type: args["page_type"],
          _sort_by: args["sort_by"],
          _filters: args["filters"],
          _limit: args["limit"] ?? 10,
        },
      };
    }
    case "cortex_auth": {
      const creds = (args["credentials"] ?? {}) as Record<string, unknown>;
      return {
        method: "auth",
        params: {
          domain: args["domain"],
          auth_type: args["method"],
          ...creds,
        },
      };
    }
    default:
      return { method: "status", params: {} };
  }
}

/** Format Cortex result as human-readable text for the LLM. */
function formatResult(
  toolName: string,
  result: Record<string, unknown>,
): string {
  switch (toolName) {
    case "cortex_map":
      return (
        `Mapped ${result["domain"] ?? "site"}: ` +
        `${result["node_count"] ?? 0} pages, ` +
        `${result["edge_count"] ?? 0} links. ` +
        `Use cortex_query to search this map.`
      );
    case "cortex_query": {
      const matches = (result["matches"] ?? []) as Record<string, unknown>[];
      if (matches.length === 0) return "No results found.";
      const lines = [`Found ${matches.length} result(s):`];
      for (const m of matches) {
        lines.push(
          `  [${m["index"]}] type=${m["page_type"]} url=${m["url"]}`,
        );
      }
      return lines.join("\n");
    }
    case "cortex_pathfind":
      if (result["nodes"]) {
        const nodes = result["nodes"] as number[];
        return (
          `Path found: ${nodes.length} steps, ` +
          `${result["hops"]} hops, weight=${result["total_weight"]}\n` +
          `Nodes: ${nodes.join(" → ")}`
        );
      }
      return "No path found between the specified nodes.";
    case "cortex_perceive":
      return (
        `Page: ${result["url"]}\n` +
        `Type: ${result["page_type"]} (confidence: ${result["confidence"]})\n` +
        `Load time: ${result["load_time_ms"]}ms` +
        (result["content"] ? `\nContent: ${result["content"]}` : "")
      );
    default:
      return JSON.stringify(result, null, 2);
  }
}

async function main(): Promise<void> {
  const client = new CortexClient(SOCKET_PATH);

  const server = new Server(
    {
      name: "cortex",
      version: "1.0.0",
    },
    {
      capabilities: {
        tools: {},
      },
    },
  );

  server.setRequestHandler(ListToolsRequestSchema, async () => ({
    tools: TOOLS,
  }));

  server.setRequestHandler(CallToolRequestSchema, async (request) => {
    const toolName = request.params.name;
    const args = (request.params.arguments ?? {}) as Record<string, unknown>;

    // Handle cortex_compare as multi-step
    if (toolName === "cortex_compare") {
      const domains = (args["domains"] ?? []) as string[];
      const allResults: Record<string, unknown>[] = [];

      for (const domain of domains) {
        try {
          // Map each domain
          await client.send("map", {
            domain,
            max_time_ms: 30000,
          });

          // Query each domain
          const queryParams: Record<string, unknown> = {
            domain,
            limit: args["limit"] ?? 10,
          };
          if (args["page_type"] != null)
            queryParams["page_type"] = args["page_type"];
          const features: Record<string, Record<string, unknown>> = {};
          const filters = (args["filters"] ?? {}) as Record<string, unknown>;
          if (filters["price_lt"] != null)
            features["48"] = { lt: filters["price_lt"] };
          if (filters["rating_gt"] != null)
            features["52"] = { gt: filters["rating_gt"] };
          if (Object.keys(features).length > 0)
            queryParams["features"] = features;

          const queryResult = await client.send("query", queryParams);
          const matches = (queryResult["matches"] ?? []) as Record<
            string,
            unknown
          >[];
          for (const m of matches) {
            allResults.push({ ...m, domain });
          }
        } catch {
          allResults.push({
            domain,
            error: `Failed to map or query ${domain}`,
          });
        }
      }

      // Sort results
      const sortBy = args["sort_by"] as string | undefined;
      if (sortBy) {
        allResults.sort((a, b) => {
          const af = (a["features"] ?? {}) as Record<string, number>;
          const bf = (b["features"] ?? {}) as Record<string, number>;
          switch (sortBy) {
            case "price_asc":
              return (af["48"] ?? Infinity) - (bf["48"] ?? Infinity);
            case "price_desc":
              return (bf["48"] ?? 0) - (af["48"] ?? 0);
            case "rating_desc":
              return (bf["52"] ?? 0) - (af["52"] ?? 0);
            case "review_count_desc":
              return (bf["56"] ?? 0) - (af["56"] ?? 0);
            default:
              return 0;
          }
        });
      }

      const limit = (args["limit"] as number) ?? 10;
      const trimmed = allResults.slice(0, limit);

      const lines = [`Compared ${domains.length} sites, ${trimmed.length} results:`];
      for (const r of trimmed) {
        if (r["error"]) {
          lines.push(`  [${r["domain"]}] ${r["error"]}`);
        } else {
          lines.push(
            `  [${r["domain"]}] node=${r["index"]} type=${r["page_type"]} url=${r["url"]}`,
          );
        }
      }

      return {
        content: [{ type: "text", text: lines.join("\n") }],
      };
    }

    // Standard single-method tools
    const { method, params } = buildParams(toolName, args);

    try {
      const result = await client.send(method, params);
      const text = formatResult(toolName, result);
      return {
        content: [{ type: "text", text }],
      };
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      return {
        content: [{ type: "text", text: `Error: ${message}` }],
        isError: true,
      };
    }
  });

  const transport = new StdioServerTransport();
  await server.connect(transport);
}

main().catch((err) => {
  console.error("Fatal:", err);
  process.exit(1);
});
