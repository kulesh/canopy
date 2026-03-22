/**
 * Curl Proxy — HTTP→HTTPS relay for sandboxed environments
 *
 * Some environments (CI sandboxes, VMs) block Node's outbound HTTPS
 * but allow curl. This proxy runs a local HTTP server that forwards
 * requests to a target HTTPS host via curl, preserving headers and
 * streaming (SSE).
 *
 * Usage:
 *   const proxy = await startCurlProxy("api.anthropic.com");
 *   // SDK calls http://localhost:<port>/v1/messages
 *   // proxy forwards to https://api.anthropic.com/v1/messages via curl
 *   proxy.stop();
 */

import * as http from "node:http";
import { spawn } from "node:child_process";

export interface CurlProxy {
  /** Local URL to use as baseURL for SDK clients. */
  baseUrl: string;
  /** Port the proxy is listening on. */
  port: number;
  /** Shut down the proxy server. */
  stop(): Promise<void>;
}

/** Headers to forward from the incoming request to curl. */
const FORWARDED_HEADERS = [
  "content-type",
  "x-api-key",
  "anthropic-version",
  "anthropic-beta",
  "anthropic-dangerous-direct-browser-access",
  "authorization",
  "accept",
];

export function startCurlProxy(targetHost: string): Promise<CurlProxy> {
  return new Promise((resolve, reject) => {
    const server = http.createServer((req, res) => {
      // CORS preflight
      if (req.method === "OPTIONS") {
        res.writeHead(204, {
          "access-control-allow-origin": "*",
          "access-control-allow-methods": "*",
          "access-control-allow-headers": "*",
          "access-control-max-age": "86400",
        });
        res.end();
        return;
      }

      let body = "";
      req.on("data", (chunk) => (body += chunk));
      req.on("end", () => {
        const targetUrl = `https://${targetHost}${req.url}`;

        const args = [
          "-s",         // silent (no progress)
          "-N",         // no buffering (stream as received)
          "--http1.1",  // force HTTP/1.1 for clean header format
          "-i",         // include response headers in stdout
          "-X", req.method ?? "GET",
        ];

        // Forward relevant headers
        for (const name of FORWARDED_HEADERS) {
          const value = req.headers[name];
          if (value) {
            args.push("-H", `${name}: ${Array.isArray(value) ? value.join(", ") : value}`);
          }
        }

        // Include body for POST/PUT/PATCH
        if (body && req.method !== "GET" && req.method !== "HEAD") {
          args.push("--data-binary", "@-");
        }

        args.push(targetUrl);

        const curl = spawn("curl", args);

        // Feed request body via stdin (avoids shell escaping issues with -d)
        if (body && req.method !== "GET" && req.method !== "HEAD") {
          curl.stdin.write(body);
          curl.stdin.end();
        }

        let headersParsed = false;
        let buffer = Buffer.alloc(0);
        const CRLF2 = Buffer.from("\r\n\r\n");

        curl.stdout.on("data", (chunk: Buffer) => {
          if (headersParsed) {
            res.write(chunk);
            return;
          }

          buffer = Buffer.concat([buffer, chunk]);

          // curl -i may produce multiple header blocks (e.g., proxy + origin).
          // Skip through all of them until we find actual body content.
          while (!headersParsed) {
            const boundary = buffer.indexOf(CRLF2);
            if (boundary === -1) return; // need more data

            const headerSection = buffer.subarray(0, boundary).toString("utf-8");
            const rest = buffer.subarray(boundary + CRLF2.length);

            // Parse this header block
            const lines = headerSection.split("\r\n");
            const statusMatch = lines[0]?.match(/^HTTP\/[\d.]+ (\d+)/);
            const statusCode = statusMatch ? parseInt(statusMatch[1], 10) : 502;

            // Check if what follows is another HTTP response (proxy layer)
            if (rest.length >= 5 && rest.subarray(0, 5).toString("utf-8").startsWith("HTTP/")) {
              // Another header block follows — skip this one, keep the rest
              buffer = rest;
              continue;
            }

            // This is the final header block — extract headers and emit body
            headersParsed = true;

            const headers: Record<string, string> = {
              "access-control-allow-origin": "*",
              "access-control-expose-headers": "*",
            };
            for (let i = 1; i < lines.length; i++) {
              const colon = lines[i].indexOf(":");
              if (colon > 0) {
                const key = lines[i].substring(0, colon).trim().toLowerCase();
                const val = lines[i].substring(colon + 1).trim();
                headers[key] = val;
              }
            }

            res.writeHead(statusCode, headers);

            if (rest.length > 0) {
              res.write(rest);
            }
          }
        });

        curl.stderr.on("data", (data: Buffer) => {
          // curl errors (not header data anymore)
          console.error(`[curl-proxy] ${data.toString().trim()}`);
        });

        curl.on("close", (code) => {
          if (!headersParsed) {
            res.writeHead(502, { "content-type": "text/plain" });
            res.end(`curl exited with code ${code}`);
          } else {
            res.end();
          }
        });

        curl.on("error", (err) => {
          if (!headersParsed) {
            res.writeHead(502, { "content-type": "text/plain" });
            res.end(`curl spawn error: ${err.message}`);
          }
        });
      });
    });

    server.listen(0, "127.0.0.1", () => {
      const addr = server.address();
      if (!addr || typeof addr === "string") {
        reject(new Error("Failed to get server address"));
        return;
      }

      const port = addr.port;
      resolve({
        baseUrl: `http://127.0.0.1:${port}`,
        port,
        stop: () =>
          new Promise<void>((res) => {
            server.close(() => res());
          }),
      });
    });

    server.on("error", reject);
  });
}
