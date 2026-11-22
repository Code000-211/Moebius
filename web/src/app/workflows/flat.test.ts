/** The workflow pages are flat: hairlines and borders, never the old card shadow. */
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

describe("the workflow pages", () => {
  it.each([
    "WorkflowsTab.tsx",
    "WorkflowPage.tsx",
    "WorkflowGraph.tsx",
    "WorkflowRunPage.tsx",
    "canvas/cards.tsx",
  ])("%s draws no shadow-card", (file) => {
    const source = readFileSync(new URL(file, import.meta.url), "utf8");
    expect(source).not.toContain("shadow-card");
  });
});

describe("the JSON block", () => {
  it("reads in the code colours, so it is legible on the code background in both themes", () => {
    const source = readFileSync("src/ui/page/JsonValue.tsx", "utf8");
    expect(source).toContain("bg-code-bg");
    expect(source).toContain("text-code-text");
    expect(source).not.toMatch(/bg-code\b(?!-)[^"]*text-text/);
  });
});
