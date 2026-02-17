// Copyright 2026 Cortex Contributors
// SPDX-License-Identifier: Apache-2.0

/**
 * Unix socket connection to the Cortex runtime.
 */

import * as net from "net";

export const DEFAULT_SOCKET_PATH = "/tmp/cortex.sock";
export const DEFAULT_TIMEOUT = 60_000;

export interface CortexResponse {
  id: string;
  result?: Record<string, unknown>;
  error?: { code?: string; message?: string };
}

/**
 * Base error for all Cortex client errors.
 * All errors include a `code` attribute for programmatic handling.
 */
export class CortexError extends Error {
  readonly code: string;
  constructor(message: string, code = "E_UNKNOWN") {
    super(message);
    this.name = "CortexError";
    this.code = code;
  }
}

export class CortexConnectionError extends CortexError {
  constructor(message: string, code = "E_CONNECTION") {
    super(message, code);
    this.name = "CortexConnectionError";
  }
}

export class CortexTimeoutError extends CortexError {
  constructor(message: string, code = "E_TIMEOUT") {
    super(message, code);
    this.name = "CortexTimeoutError";
  }
}

export class CortexMapError extends CortexError {
  constructor(message: string, code = "E_MAP_FAILED") {
    super(message, code);
    this.name = "CortexMapError";
  }
}

export class CortexPathError extends CortexError {
  constructor(message: string, code = "E_PATH_FAILED") {
    super(message, code);
    this.name = "CortexPathError";
  }
}

export class CortexActError extends CortexError {
  constructor(message: string, code = "E_ACT_FAILED") {
    super(message, code);
    this.name = "CortexActError";
  }
}

/** Feature vector dimension count. */
export const FEATURE_DIM = 128;

/**
 * Normalize a domain input by stripping protocol, path, and trailing slashes.
 */
export function normalizeDomain(domain: string): string {
  let d = domain.trim();
  if (!d) throw new Error("domain cannot be empty");

  // Strip protocol
  if (d.includes("://")) {
    try {
      const url = new URL(d);
      d = url.host;
    } catch {
      d = d.split("://")[1]?.split("/")[0] ?? d;
    }
  } else if (d.startsWith("//")) {
    d = d.slice(2).split("/")[0];
  } else {
    d = d.split("/")[0];
  }

  // Strip trailing dots
  d = d.replace(/\.+$/, "");

  if (!d) throw new Error(`domain cannot be empty (input was "${domain}")`);

  return d;
}

export class Connection {
  private socketPath: string;
  private timeout: number;
  private socket: net.Socket | null = null;
  private buffer = "";
  private requestId = 0;

  constructor(
    socketPath: string = DEFAULT_SOCKET_PATH,
    timeout: number = DEFAULT_TIMEOUT
  ) {
    this.socketPath = socketPath;
    this.timeout = timeout;
  }

  /**
   * Connect to the Cortex runtime socket.
   */
  connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.socket = net.createConnection({ path: this.socketPath }, () => {
        resolve();
      });
      this.socket.setTimeout(this.timeout);
      this.socket.on("error", (err: NodeJS.ErrnoException) => {
        if (err.code === "ENOENT") {
          reject(
            new CortexConnectionError(
              `Cannot connect to Cortex at ${this.socketPath}. ` +
                "The process may not be running. Start it with: cortex start",
              "E_SOCKET_NOT_FOUND"
            )
          );
        } else if (err.code === "ECONNREFUSED") {
          reject(
            new CortexConnectionError(
              `Cortex refused connection at ${this.socketPath}. ` +
                "The process may have crashed. Try 'cortex stop && cortex start'.",
              "E_CONNECTION_REFUSED"
            )
          );
        } else if (err.code === "EACCES") {
          reject(
            new CortexConnectionError(
              `Permission denied on ${this.socketPath}. ` +
                "Check file permissions or run 'cortex stop && cortex start'.",
              "E_PERMISSION_DENIED"
            )
          );
        } else {
          reject(
            new CortexConnectionError(`Cannot connect to Cortex: ${err.message}`)
          );
        }
      });
      this.socket.on("timeout", () => {
        reject(
          new CortexTimeoutError(
            "Connection timed out. The Cortex daemon may be overloaded."
          )
        );
      });
    });
  }

  /**
   * Close the connection.
   */
  close(): void {
    if (this.socket) {
      this.socket.destroy();
      this.socket = null;
    }
    this.buffer = "";
  }

  /**
   * Check if connected.
   */
  get isConnected(): boolean {
    return this.socket !== null && !this.socket.destroyed;
  }

  /**
   * Send a request and return the response.
   */
  async send(
    method: string,
    params: Record<string, unknown> = {}
  ): Promise<CortexResponse> {
    if (!this.socket || this.socket.destroyed) {
      await this.connect();
    }

    const id = `req-${++this.requestId}`;
    const request = JSON.stringify({ id, method, params }) + "\n";

    return new Promise((resolve, reject) => {
      const sock = this.socket!;

      const timeoutId = setTimeout(() => {
        cleanup();
        reject(
          new CortexTimeoutError(
            `Timeout sending ${method} request after ${this.timeout}ms.`
          )
        );
      }, this.timeout);

      const onData = (chunk: Buffer): void => {
        this.buffer += chunk.toString("utf-8");
        const newlineIdx = this.buffer.indexOf("\n");
        if (newlineIdx !== -1) {
          const line = this.buffer.substring(0, newlineIdx);
          this.buffer = this.buffer.substring(newlineIdx + 1);
          cleanup();
          try {
            resolve(JSON.parse(line) as CortexResponse);
          } catch {
            reject(
              new CortexConnectionError(
                "Invalid JSON response from Cortex daemon.",
                "E_INVALID_JSON"
              )
            );
          }
        }
      };

      const onError = (err: Error): void => {
        cleanup();
        reject(
          new CortexConnectionError(
            `Connection error: ${err.message}. ` +
              "The Cortex daemon may have crashed."
          )
        );
      };

      const onClose = (): void => {
        cleanup();
        reject(
          new CortexConnectionError(
            "Connection closed by Cortex daemon. " +
              "Check 'cortex doctor' for diagnostics.",
            "E_CONNECTION_CLOSED"
          )
        );
      };

      const cleanup = (): void => {
        clearTimeout(timeoutId);
        sock.removeListener("data", onData);
        sock.removeListener("error", onError);
        sock.removeListener("close", onClose);
      };

      sock.on("data", onData);
      sock.on("error", onError);
      sock.on("close", onClose);

      sock.write(request, (err) => {
        if (err) {
          cleanup();
          reject(
            new CortexConnectionError(
              `Write error: ${err.message}`,
              "E_WRITE_FAILED"
            )
          );
        }
      });
    });
  }
}
