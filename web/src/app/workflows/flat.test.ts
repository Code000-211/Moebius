/** The workflow pages are flat: hairlines and borders, never the old card shadow. */
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

describe("the workflow pages", () => {
  it.each([
    "WorkflowsPage.tsx",
    "WorkflowPage.tsx",
    "WorkflowGraph.tsx",
    "WorkflowRunPage.tsx",
    "canvas/cards.tsx",
  ])("%s draws no shadow-card", (file) => {
    const source = readFileSync(new URL(file, import.meta.url), "utf8");
    expect(source).not.toContain("shadow-card");
  });
});
