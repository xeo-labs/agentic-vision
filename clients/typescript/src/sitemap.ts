// Copyright 2026 Cortex Contributors
// SPDX-License-Identifier: Apache-2.0

/**
 * SiteMap types and query interface.
 *
 * @example
 * ```typescript
 * const site = await map("amazon.com");
 * const products = await site.filter({ pageType: 0x04, limit: 5 });
 * const path = await site.pathfind(0, products[0].index);
 * ```
 */

import {
  Connection,
  CortexActError,
  CortexPathError,
  CortexResponse,
  FEATURE_DIM,
} from "./connection";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface NodeMatch {
  index: number;
  url: string;
  pageType: number;
  confidence: number;
  features: Record<number, number>;
  similarity?: number;
}

export interface PathAction {
  atNode: number;
  opcode: [number, number];
}

export interface Path {
  nodes: number[];
  totalWeight: number;
  hops: number;
  requiredActions: PathAction[];
}

export interface RefreshResult {
  updatedCount: number;
  changedNodes: number[];
}

export interface ActResult {
  success: boolean;
  newUrl?: string;
  features: Record<number, number>;
}

export interface WatchDelta {
  node: number;
  changedFeatures: Record<number, [number, number]>;
  timestamp: number;
}

export interface NodeQuery {
  pageType?: number | number[];
  features?: Record<number, { min?: number; max?: number }>;
  flags?: Record<string, boolean>;
  sortBy?: { dimension: number; direction: string };
  limit?: number;
}

// ---------------------------------------------------------------------------
// Protocol helpers
// ---------------------------------------------------------------------------

function buildQueryParams(
  domain: string,
  query: NodeQuery = {}
): Record<string, unknown> {
  const params: Record<string, unknown> = {
    domain,
    limit: query.limit ?? 100,
  };
  if (query.pageType !== undefined) {
    params.page_type = Array.isArray(query.pageType)
      ? query.pageType
      : [query.pageType];
  }
  if (query.features) params.features = query.features;
  if (query.flags) params.flags = query.flags;
  if (query.sortBy) {
    params.sort_by = {
      dimension: query.sortBy.dimension,
      direction: query.sortBy.direction,
    };
  }
  return params;
}

function parseNodeMatches(resp: CortexResponse): NodeMatch[] {
  if (resp.error) {
    throw new Error(resp.error.message ?? "query error");
  }
  const result = (resp.result ?? {}) as Record<string, unknown>;
  const matches = (result.matches ?? []) as Array<Record<string, unknown>>;
  return matches.map((m) => ({
    index: (m.index as number) ?? 0,
    url: (m.url as string) ?? "",
    pageType: (m.page_type as number) ?? 0,
    confidence: (m.confidence as number) ?? 0,
    features: (m.features as Record<number, number>) ?? {},
    similarity: m.similarity as number | undefined,
  }));
}

// ---------------------------------------------------------------------------
// SiteMapClient
// ---------------------------------------------------------------------------

/**
 * Navigable binary site map client.
 *
 * Wraps protocol responses to provide a convenient query interface.
 */
export class SiteMapClient {
  private conn: Connection;
  readonly domain: string;
  readonly nodeCount: number;
  readonly edgeCount: number;
  readonly mapPath?: string;

  constructor(
    conn: Connection,
    domain: string,
    nodeCount: number,
    edgeCount: number,
    mapPath?: string
  ) {
    this.conn = conn;
    this.domain = domain;
    this.nodeCount = nodeCount;
    this.edgeCount = edgeCount;
    this.mapPath = mapPath;
  }

  toString(): string {
    return `SiteMap(domain='${this.domain}', nodes=${this.nodeCount}, edges=${this.edgeCount})`;
  }

  /**
   * Filter nodes by type, features, and flags.
   *
   * @returns Array of matching nodes (empty if no matches, never null).
   */
  async filter(query: NodeQuery = {}): Promise<NodeMatch[]> {
    const params = buildQueryParams(this.domain, query);
    const resp = await this.conn.send("query", params);
    return parseNodeMatches(resp);
  }

  /**
   * Find k nearest nodes by cosine similarity to a goal vector.
   *
   * @param goalVector - A 128-dimension feature vector to compare against.
   * @param k - Number of nearest neighbors to return.
   * @throws Error if goalVector is not exactly 128 dimensions.
   */
  async nearest(goalVector: number[], k = 10): Promise<NodeMatch[]> {
    if (goalVector.length !== FEATURE_DIM) {
      throw new Error(
        `Goal vector must be ${FEATURE_DIM} dimensions, got ${goalVector.length}`
      );
    }
    const params = buildQueryParams(this.domain, { limit: k });
    params.goal_vector = goalVector;
    params.mode = "nearest";
    const resp = await this.conn.send("query", params);
    return parseNodeMatches(resp);
  }

  /**
   * Find shortest path between two nodes.
   *
   * @returns Path object, or null if no path exists.
   */
  async pathfind(
    fromNode: number,
    toNode: number,
    options: { avoidFlags?: string[]; minimize?: string } = {}
  ): Promise<Path | null> {
    const params: Record<string, unknown> = {
      domain: this.domain,
      from: fromNode,
      to: toNode,
      minimize: options.minimize ?? "hops",
    };
    if (options.avoidFlags) params.avoid_flags = options.avoidFlags;

    const resp = await this.conn.send("pathfind", params);

    if (resp.error) {
      if (resp.error.code === "E_NO_PATH") return null;
      throw new CortexPathError(
        resp.error.message ?? "pathfind error",
        resp.error.code ?? "E_PATH_FAILED"
      );
    }

    const result = (resp.result ?? {}) as Record<string, unknown>;
    const actions = (
      (result.required_actions ?? []) as Array<Record<string, unknown>>
    ).map((a) => ({
      atNode: a.at_node as number,
      opcode: a.opcode as [number, number],
    }));

    return {
      nodes: (result.nodes as number[]) ?? [],
      totalWeight: (result.total_weight as number) ?? 0,
      hops: (result.hops as number) ?? 0,
      requiredActions: actions,
    };
  }

  /**
   * Re-render specific nodes and update the map.
   */
  async refresh(
    options: {
      nodes?: number[];
      cluster?: number;
      staleThreshold?: number;
    } = {}
  ): Promise<RefreshResult> {
    const params: Record<string, unknown> = { domain: this.domain };
    if (options.nodes !== undefined) params.nodes = options.nodes;
    if (options.cluster !== undefined) params.cluster = options.cluster;
    if (options.staleThreshold !== undefined)
      params.stale_threshold = options.staleThreshold;

    const resp = await this.conn.send("refresh", params);
    const result = (resp.result ?? {}) as Record<string, unknown>;
    return {
      updatedCount: (result.updated_count as number) ?? 0,
      changedNodes: (result.changed_nodes as number[]) ?? [],
    };
  }

  /**
   * Execute an action on a live page.
   *
   * @throws CortexActError if the action fails.
   */
  async act(
    node: number,
    opcode: [number, number],
    params?: Record<string, unknown>,
    sessionId?: string
  ): Promise<ActResult> {
    const reqParams: Record<string, unknown> = {
      domain: this.domain,
      node,
      opcode,
    };
    if (params) reqParams.params = params;
    if (sessionId) reqParams.session_id = sessionId;

    const resp = await this.conn.send("act", reqParams);
    if (resp.error) {
      throw new CortexActError(
        resp.error.message ?? "action failed",
        resp.error.code ?? "E_ACT_FAILED"
      );
    }

    const result = (resp.result ?? {}) as Record<string, unknown>;
    return {
      success: (result.success as boolean) ?? false,
      newUrl: result.new_url as string | undefined,
      features: (result.features as Record<number, number>) ?? {},
    };
  }
}
