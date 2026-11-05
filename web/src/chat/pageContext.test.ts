import { describe, expect, it } from "vitest";
import type { Snapshot } from "../api/types/board";
import { pageContext } from "./pageContext";

const snapshot = {
  boards: [{ repository: { id: 3, name: "demo" } }],
} as unknown as Snapshot;

describe("pageContext", () => {
  it("tells the agent a project's page is about its settings, not its code", () => {
    const context = pageContext("/projects/3", snapshot);
    expect(context.text).toContain("Repository: demo (id 3)");
    expect(context.text).toContain("settings");
    expect(context.text).toContain("not the code");
    expect(context.name).toBe("demo");
    // A chat started here does not work in the repository's worktree.
    expect(context.repository).toBeNull();
  });

  it("tells the agent the settings pages are about Moebius's own setup", () => {
    for (const path of ["/settings", "/plugin-settings"]) {
      expect(pageContext(path, snapshot).text).toContain("sets up Moebius itself");
    }
    expect(pageContext("/models", snapshot).text).not.toContain("sets up Moebius itself");
  });
});
