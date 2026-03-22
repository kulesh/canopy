import tailwindcss from "@tailwindcss/vite";
import { VitePWA } from "vite-plugin-pwa";
import { defineConfig, type Plugin } from "vite";
import http from "http";
import https from "https";
import { URL } from "url";

/**
 * CORS proxy plugin for Vite dev server.
 *
 * The Pi SDK's proxy format: `<proxy-url>/?url=<encoded-target-url>`
 * This plugin handles `/cors-proxy/?url=...` requests by forwarding them
 * to the target URL server-side, bypassing browser CORS restrictions.
 */
function corsProxy(): Plugin {
  return {
    name: "cors-proxy",
    configureServer(server) {
      server.middlewares.use("/cors-proxy", (req, res) => {
        const reqUrl = new URL(req.url ?? "/", "http://localhost");
        const targetStr = reqUrl.searchParams.get("url");

        if (!targetStr) {
          res.writeHead(400, { "Content-Type": "text/plain" });
          res.end("Missing ?url= parameter");
          return;
        }

        let target: URL;
        try {
          target = new URL(targetStr);
        } catch {
          res.writeHead(400, { "Content-Type": "text/plain" });
          res.end("Invalid target URL");
          return;
        }

        // Build the proxied path: target path + any additional path from the request
        const extraPath = reqUrl.pathname.replace(/^\/cors-proxy\/?/, "");
        const fullPath = target.pathname + extraPath + (target.search || "");

        const transport = target.protocol === "https:" ? https : http;
        const headers = { ...req.headers, host: target.host };
        delete headers["origin"];
        delete headers["referer"];

        const proxyReq = transport.request(
          {
            hostname: target.hostname,
            port: target.port,
            path: fullPath,
            method: req.method,
            headers,
          },
          (proxyRes) => {
            // Add CORS headers so the browser accepts the response
            res.writeHead(proxyRes.statusCode ?? 500, {
              ...proxyRes.headers,
              "access-control-allow-origin": "*",
              "access-control-allow-methods": "*",
              "access-control-allow-headers": "*",
              "access-control-expose-headers": "*",
            });
            proxyRes.pipe(res);
          },
        );

        proxyReq.on("error", (err) => {
          res.writeHead(502, { "Content-Type": "text/plain" });
          res.end(`Proxy error: ${err.message}`);
        });

        req.pipe(proxyReq);
      });

      // Handle CORS preflight
      server.middlewares.use("/cors-proxy", (req, res, next) => {
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
        next();
      });
    },
  };
}

export default defineConfig({
  plugins: [
    tailwindcss(),
    corsProxy(),
    VitePWA({
      registerType: "autoUpdate",
      workbox: {
        globPatterns: ["**/*.{js,css,html,svg,png,woff2}"],
        maximumFileSizeToCacheInBytes: 5 * 1024 * 1024,
        runtimeCaching: [
          {
            urlPattern: /^https:\/\/api\.anthropic\.com\/.*/,
            handler: "NetworkOnly",
          },
        ],
      },
      manifest: false, // We manage our own manifest.json
    }),
  ],
  build: {
    target: "esnext",
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes("pdfjs-dist")) return "vendor-pdf";
          if (id.includes("pi-web-ui") || id.includes("mini-lit") || id.includes("/lit/"))
            return "vendor-pi-ui";
          // Keep provider modules (anthropic, mistral, etc.) as separate lazy chunks
          if (id.includes("pi-ai/dist/providers")) return;
          if (id.includes("pi-ai") || id.includes("pi-agent-core"))
            return "vendor-pi-ai";
        },
      },
    },
  },
  server: {
    port: 3100,
  },
  test: {
    include: ["tests/**/*.test.ts"],
  },
});
