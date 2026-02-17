// Copyright 2026 Cortex Contributors
// SPDX-License-Identifier: Apache-2.0

/**
 * @cortex-ai/client — Thin client for the Cortex web cartography runtime.
 */

export {
  Connection,
  CortexConnectionError,
  CortexTimeoutError,
  DEFAULT_SOCKET_PATH,
  DEFAULT_TIMEOUT,
} from "./connection";

export type { CortexResponse } from "./connection";

export {
  SiteMapClient,
} from "./sitemap";

export type {
  NodeMatch,
  Path,
  PathAction,
  RefreshResult,
  ActResult,
  WatchDelta,
  NodeQuery,
} from "./sitemap";

export {
  map,
  mapMany,
  perceive,
  perceiveMany,
  status,
} from "./client";

export type {
  MapOptions,
  PerceiveOptions,
  RuntimeStatus,
  PageResult,
} from "./client";
