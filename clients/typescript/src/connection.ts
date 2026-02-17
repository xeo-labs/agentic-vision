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

export class CortexConnectionError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "CortexConnectionError";
  }
}

export class CortexTimeoutError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "CortexTimeoutError";
  }
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
              `Cortex is not running (socket not found: ${this.socketPath})`
            )
          );
        } else if (err.code === "ECONNREFUSED") {
          reject(
            new CortexConnectionError(
              `Cortex refused connection at ${this.socketPath}`
            )
          );
        } else {
          reject(new CortexConnectionError(`Cannot connect to Cortex: ${err.message}`));
        }
      });
      this.socket.on("timeout", () => {
        reject(new CortexTimeoutError("Connection timed out"));
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
        reject(new CortexTimeoutError(`Timeout sending ${method} request`));
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
            reject(new CortexConnectionError("Invalid JSON response"));
          }
        }
      };

      const onError = (err: Error): void => {
        cleanup();
        reject(new CortexConnectionError(`Connection error: ${err.message}`));
      };

      const onClose = (): void => {
        cleanup();
        reject(new CortexConnectionError("Connection closed by server"));
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
          reject(new CortexConnectionError(`Write error: ${err.message}`));
        }
      });
    });
  }
}
