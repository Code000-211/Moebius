import { describe, expect, it } from "vitest";
import {
  inSettingsArea,
  NAV,
  SETTINGS_GROUPS,
  SIDEBAR_GROUPS,
  searchEntries,
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

  it("lists Cycles, Milestones, Intake, Workflows and Automations nowhere: they live in projects", () => {
    const listed = new Set(NAV.filter((e) => e.sidebar !== undefined).map((e) => e.path));
    for (const path of ["cycles", "milestones", "intake", "workflows", "automations", "more"])
      expect(listed.has(path), path).toBe(false);
  });
});

describe("the Settings navigation", () => {
  it("keeps the groups in their order and Members for an owner alone", () => {
    const owner = settingsEntries("owner");
    expect(searchEntries(owner, "").map((g) => g.group)).toEqual([...SETTINGS_GROUPS]);
    expect(owner.map((e) => e.label)).toContain("Members");
    for (const role of ["member", "viewer", undefined])
      expect(settingsEntries(role).map((e) => e.label)).not.toContain("Members");
  });

  it("names the page of /settings General and keeps Keyboard, Analytics and Retro under their names", () => {
    const labels = Object.fromEntries(settingsEntries("owner").map((e) => [e.path, e.label]));
    expect(labels.settings).toBe("General");
    expect(labels.shortcuts).toBe("Keyboard");
    expect(labels.analytics).toBe("Analytics");
    expect(labels.retro).toBe("Retro");
  });

  it("finds an entry by its name or its group's", () => {
    const owner = settingsEntries("owner");
    expect(searchEntries(owner, " RUL ")[0]?.entries.map((e) => e.label)).toEqual(["Rules"]);
    expect(searchEntries(owner, "zzz")).toEqual([]);
  });

  it("chooses Settings on its own pages, not on a page the sidebar has a row for", () => {
    for (const path of ["/profile", "/settings", "/tokens", "/usage"])
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
      "/analytics",
      "/retro",
    ])
      expect(inSettingsArea(path), path).toBe(false);
  });
});
