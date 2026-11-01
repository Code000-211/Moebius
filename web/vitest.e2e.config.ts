/** The end-to-end tests: Vitest as the runner for Playwright's library, in Node, one file at a time. */
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "node",
    include: ["e2e/**/*.e2e.ts"],
    globalSetup: ["e2e/global-setup.ts"],
    testTimeout: 60_000,
    hookTimeout: 120_000,
    fileParallelism: false,
  },
});
