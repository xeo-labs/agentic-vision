// Copyright 2026 Cortex Contributors
// SPDX-License-Identifier: Apache-2.0

/**
 * High-level Cortex client functions.
 *
 * @example
 * ```typescript
 * import { map } from "@cortex-ai/client";
 *
 * const site = await map("amazon.com");
 * const products = await site.filter({ pageType: 0x04, limit: 5 });
 * ```
 */

import {
  Connection,
  CortexMapError,
  DEFAULT_SOCKET_PATH,
  normalizeDomain,
} from "./connection";
import { SiteMapClient } from "./sitemap";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface MapOptions {
  maxNodes?: number;
  maxRender?: number;
  maxTimeMs?: number;
  respectRobots?: boolean;
  socketPath?: string;
  /** Client timeout in milliseconds. */
  timeoutMs?: number;
}

export interface PerceiveOptions {
  includeContent?: boolean;
  socketPath?: string;
}

export interface RuntimeStatus {
  version: string;
  uptimeSeconds: number;
  activeContexts: number;
  cachedMaps: number;
  memoryMb: number;
}

export interface PageResult {
  url: string;
  finalUrl: string;
  pageType: number;
  confidence: number;
  features: Record<number, number>;
  content?: string;
}

// ---------------------------------------------------------------------------
// Functions
// ---------------------------------------------------------------------------

/**
 * Map a website and return a navigable SiteMapClient.
 *
 * Accepts full URLs — protocol and path will be stripped automatically.
 *
 * @param domain - Domain to map (e.g. "amazon.com" or "https://amazon.com/dp/...")
 * @param options - Mapping options
 * @returns Navigable site map client
 *
 * @example
 * ```typescript
 * const site = await map("amazon.com");
 * console.log(`${site.nodeCount} nodes, ${site.edgeCount} edges`);
 * ```
 */
export async function map(
  domain: string,
  options: MapOptions = {}
): Promise<SiteMapClient> {
  const normalizedDomain = normalizeDomain(domain);
  const socketPath = options.socketPath ?? DEFAULT_SOCKET_PATH;
  const conn = new Connection(socketPath, options.timeoutMs);

  const params: Record<string, unknown> = {
    domain: normalizedDomain,
    max_nodes: options.maxNodes ?? 50_000,
    max_render: options.maxRender ?? 200,
    max_time_ms: options.maxTimeMs ?? 10_000,
    respect_robots: options.respectRobots ?? true,
  };

  const resp = await conn.send("map", params);
  if (resp.error) {
    throw new CortexMapError(
      resp.error.message ?? "map failed",
      resp.error.code ?? "E_MAP_FAILED"
    );
  }

  const result = (resp.result ?? {}) as Record<string, unknown>;
  return new SiteMapClient(
    conn,
    normalizedDomain,
    (result.node_count as number) ?? 0,
    (result.edge_count as number) ?? 0,
    result.map_path as string | undefined
  );
}

/**
 * Map multiple websites concurrently.
 */
export async function mapMany(
  domains: string[],
  options: MapOptions = {}
): Promise<SiteMapClient[]> {
  return Promise.all(domains.map((d) => map(d, options)));
}

/**
 * Perceive a single page and return its encoding.
 *
 * @param url - Full URL to perceive
 * @param options - Perceive options
 */
export async function perceive(
  url: string,
  options: PerceiveOptions = {}
): Promise<PageResult> {
  const socketPath = options.socketPath ?? DEFAULT_SOCKET_PATH;
  const conn = new Connection(socketPath);

  const params: Record<string, unknown> = {
    url,
    include_content: options.includeContent ?? true,
  };

  const resp = await conn.send("perceive", params);
  if (resp.error) {
    throw new Error(resp.error.message ?? "perceive failed");
  }

  const result = (resp.result ?? {}) as Record<string, unknown>;
  return {
    url,
    finalUrl: (result.final_url as string) ?? url,
    pageType: (result.page_type as number) ?? 0,
    confidence: (result.confidence as number) ?? 0,
    features: (result.features as Record<number, number>) ?? {},
    content: result.content as string | undefined,
  };
}

/**
 * Perceive multiple pages concurrently.
 */
export async function perceiveMany(
  urls: string[],
  options: PerceiveOptions = {}
): Promise<PageResult[]> {
  return Promise.all(urls.map((u) => perceive(u, options)));
}

/**
 * Get Cortex runtime status.
 */
export async function status(
  socketPath: string = DEFAULT_SOCKET_PATH
): Promise<RuntimeStatus> {
  const conn = new Connection(socketPath);
  const resp = await conn.send("status");

  if (resp.error) {
    throw new Error(resp.error.message ?? "status failed");
  }

  const result = (resp.result ?? {}) as Record<string, unknown>;
  return {
    version: (result.version as string) ?? "unknown",
    uptimeSeconds: (result.uptime_seconds as number) ?? 0,
    activeContexts: (result.active_contexts as number) ?? 0,
    cachedMaps: (result.cached_maps as number) ?? 0,
    memoryMb: (result.memory_mb as number) ?? 0,
  };
}
