import { describe, expect, it } from "vitest";
import {
  groupEntries,
  inSettingsArea,
  NAV,
  SETTINGS_GROUPS,
  SIDEBAR_GROUPS,
  settingsEntries,
  sidebarRows,
} from "./nav";
import { routes } from "./routes";

/** The sidebar as a list of "Group: Row" lines, the first group written without its name. */
function sidebarOf(role: string | undefined): string[] {
  return SIDEBAR_GROUPS.flatMap((group) =>
    sidebarRows(group, role).map((entry) => `${group || "-"}: ${entry.label}`),
  );
}

describe("the table of places", () => {
  it("holds each path once", () => {
    const paths = NAV.map((entry) => entry.path);
    expect(new Set(paths).size).toBe(paths.length);
  });

  it("has a route for every path", () => {
    const shell = routes.find((route) => route.path === "/");
    const children = (shell?.children ?? []) as Array<{ path?: string; children?: unknown[] }>;
    const paths = new Set<string>();
    const collect = (list: Array<{ path?: string; children?: unknown[] }>) => {
      for (const route of list) {
        if (route.path) paths.add(route.path);
        if (route.children) collect(route.children as typeof list);
      }
    };
    collect(children);
    expect(NAV.map((entry) => entry.path).filter((path) => !paths.has(path))).toEqual([]);
  });
});

describe("the sidebar's rows", () => {
  it("are, for an owner, the list of the approved mockup without the workflows the project owns", () => {
    expect(sidebarOf("owner")).toEqual([
      "-: Inbox",
      "-: My issues",
      "-: Fleet",
      "-: Chat",
      "Projects: Projects",
      "Workspace: Wiki",
      "Workspace: Views",
      "Build: Agents",
      "Build: Rules",
      "Insight: Insights",
      "Insight: Improve",
    ]);
  });

  it("leave out Build and Insight for anyone who is not an owner", () => {
    for (const role of ["member", "viewer", undefined]) {
      expect(sidebarOf(role)).toEqual([
        "-: Inbox",
        "-: My issues",
        "-: Fleet",
        "-: Chat",
        "Projects: Projects",
        "Workspace: Wiki",
        "Workspace: Views",
      ]);
    }
  });

  it("lists Skills and MCP servers as tabs of Agents, to an owner alone, and none of the four in Settings", () => {
    const listed = new Set(NAV.filter((e) => e.sidebar !== undefined).map((e) => e.path));
    expect(listed.has("agents/skills")).toBe(false);
    expect(listed.has("agents/mcp-servers")).toBe(false);
    const owner = settingsEntries("owner").map((e) => e.label);
    for (const gone of ["Agents", "Skills", "MCP servers", "Rules"])
      expect(owner, gone).not.toContain(gone);
    for (const path of ["agents/skills", "agents/mcp-servers"]) {
      const entry = NAV.find((e) => e.path === path);
      expect(entry?.shownTo).toBe("owner");
      expect(inSettingsArea(`/${path}`), path).toBe(false);
    }
  });

  it("lists Insights and Improve in the Insight group to an owner alone, with their tabs and none in Settings", () => {
    expect(sidebarRows("Insight", "owner").map((e) => e.label)).toEqual(["Insights", "Improve"]);
    expect(sidebarRows("Insight", "member")).toEqual([]);
    const owner = settingsEntries("owner").map((e) => e.label);
    for (const gone of ["Analytics", "Retro", "Lessons", "Export", "Usage"])
      expect(owner, gone).not.toContain(gone);
    expect(SETTINGS_GROUPS).not.toContain("Insight");
    for (const path of [
      "insights/usage",
      "improve/lessons",
      "improve/experiments",
      "improve/decisions",
    ]) {
      const entry = NAV.find((e) => e.path === path);
      expect(entry?.shownTo, path).toBe("owner");
      expect(entry?.sidebar, path).toBeUndefined();
    }
    for (const old of [
      "analytics",
      "retro",
      "lessons",
      "export",
      "experiments",
      "decision-model",
      "usage",
    ])
      expect(
        NAV.some((e) => e.path === old),
        old,
      ).toBe(false);
  });

  it("lists Cycles, Milestones, Intake, Workflows and Automations nowhere: they live in projects", () => {
    const listed = new Set(NAV.filter((e) => e.sidebar !== undefined).map((e) => e.path));
    for (const path of ["cycles", "milestones", "intake", "workflows", "automations", "more"])
      expect(listed.has(path), path).toBe(false);
  });
});

describe("the Settings navigation", () => {
  it("keeps the groups in their order and Members for an owner alone", () => {
    const owner = settingsEntries("owner");
    expect(groupEntries(owner).map((g) => g.group)).toEqual([...SETTINGS_GROUPS]);
    expect(owner.map((e) => e.label)).toContain("Members");
    for (const role of ["member", "viewer", undefined])
      expect(settingsEntries(role).map((e) => e.label)).not.toContain("Members");
  });

  it("names the page of /settings General and keeps Keyboard shortcuts under its name", () => {
    const labels = Object.fromEntries(settingsEntries("owner").map((e) => [e.path, e.label]));
    expect(labels.settings).toBe("General");
    expect(labels.shortcuts).toBe("Keyboard shortcuts");
  });

  it("gives every entry an icon", () => {
    for (const entry of settingsEntries("owner")) expect(entry.icon, entry.label).toBeTruthy();
  });

  it("chooses Settings on its own pages, not on a page the sidebar has a row for", () => {
    for (const path of ["/profile", "/settings", "/tokens"])
      expect(inSettingsArea(path), path).toBe(true);
    for (const path of [
      "/inbox",
      "/fleet",
      "/fleet/ports",
      "/runners",
      "/ports",
      "/projects/3",
      "/agents",
      "/rules",
      "/insights",
      "/insights/usage",
      "/improve",
      "/improve/decisions",
    ])
      expect(inSettingsArea(path), path).toBe(false);
  });
});
