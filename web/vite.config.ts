import tailwindcss from "@tailwindcss/vite";
import { VitePWA } from "vite-plugin-pwa";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [
    tailwindcss(),
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
  server: {
    port: 3100,
  },
  test: {
    include: ["tests/**/*.test.ts"],
  },
});
