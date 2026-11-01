/**
 * Vite builds the app into `dist/`, which the server embeds, and in development proxies the API, the event WebSocket
 * included, to a running server: `MOEBIUS_URL`, by default `http://127.0.0.1:7171`.
 */
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

const target = process.env.MOEBIUS_URL ?? "http://127.0.0.1:7171";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    proxy: {
      "/v1": { target, ws: true, changeOrigin: false },
    },
  },
  build: { outDir: "dist", emptyOutDir: true, sourcemap: false },
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.{ts,tsx}"],
    setupFiles: ["src/test-setup.ts"],
  },
});
