/**
 * The pictures of the README: Vitest runs `e2e/screenshots/readme.test.ts`, which `pnpm e2e` does not pick up. Like the
 * end-to-end tests it opens the built app, so `pnpm build` comes first; `pnpm screenshots` runs it.
 */
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "node",
    include: ["e2e/screenshots/**/*.test.ts"],
    globalSetup: ["e2e/global-setup.ts"],
    testTimeout: 300_000,
    hookTimeout: 120_000,
    fileParallelism: false,
  },
});
