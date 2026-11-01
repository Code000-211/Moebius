/**
 * The design audit: Vitest runs the Playwright audit in `e2e/design/`, which `pnpm e2e` does not pick up. Like the
 * end-to-end tests it opens the built app, so `pnpm build` comes first; `pnpm audit:design` runs it.
 */
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "node",
    include: ["e2e/design/**/*.test.ts"],
    globalSetup: ["e2e/global-setup.ts"],
    testTimeout: 900_000,
    hookTimeout: 120_000,
    fileParallelism: false,
  },
});
