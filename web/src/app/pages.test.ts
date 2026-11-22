import { describe, expect, it } from "vitest";
import { MORE_PATHS } from "./more/MorePage";
import { PAGES, pageByPath, SETTINGS_PAGES, shownTo } from "./pages";
import { SETTINGS_ENTRIES } from "./settings/settingsEntries";

describe("the pages only an owner is shown", () => {
  it("leaves People out for a member and a viewer and keeps a person's own tokens for everyone", () => {
    const people = pageByPath("people");
    const tokens = pageByPath("tokens");
    expect(people && tokens).toBeTruthy();
    if (!people || !tokens) return;
    expect(shownTo(people, "owner")).toBe(true);
    expect(shownTo(people, "member")).toBe(false);
    expect(shownTo(people, "viewer")).toBe(false);
    for (const role of ["owner", "member", "viewer"]) expect(shownTo(tokens, role)).toBe(true);
    expect(SETTINGS_PAGES).toContain(people);
    expect(SETTINGS_PAGES).toContain(tokens);
  });
});

describe("the reach of every page", () => {
  /** Pages that are on no list on purpose, each with the reason. */
  const EXCEPTIONS: Record<string, string> = {
    issues: "the all-issues list is reached from the command palette and the project pages",
    workflows: "a project's Workflows tab, which the command palette and the old address lead to",
  };

  it("leaves no page orphaned: each is in the sidebar, Settings, More or a named exception", () => {
    const sidebar = new Set(
      PAGES.filter((page) => ["", "Workspace", "More"].includes(page.group)).map((p) => p.path),
    );
    sidebar.add("projects");
    sidebar.add("settings");
    const reached = new Set<string>([
      ...sidebar,
      ...SETTINGS_ENTRIES.map((entry) => entry.path),
      ...MORE_PATHS,
      ...Object.keys(EXCEPTIONS),
    ]);
    expect(PAGES.map((page) => page.path).filter((path) => !reached.has(path))).toEqual([]);
  });
});
