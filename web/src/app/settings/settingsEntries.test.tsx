import { describe, expect, it } from "vitest";
import {
  inSettingsArea,
  SETTINGS_ENTRIES,
  SETTINGS_GROUPS,
  searchEntries,
  shownEntries,
} from "./settingsEntries";

describe("settings entries", () => {
  it("holds the twenty decided entries, in five groups", () => {
    expect(SETTINGS_ENTRIES).toHaveLength(20);
    expect(SETTINGS_GROUPS).toEqual(["You", "Agents", "Automation", "Insight", "Server"]);
    expect(new Set(SETTINGS_ENTRIES.map((e) => e.path)).size).toBe(20);
  });

  it("lists the entries that still have no page, which later tickets must remove", () => {
    expect(SETTINGS_ENTRIES.filter((e) => !e.page).map((e) => e.label)).toEqual([
      "Column scripts",
      "Schedules",
      "Export",
      "Runners",
    ]);
  });

  it("shows registered entries only, and the owner's entries to an owner only", () => {
    expect(shownEntries("owner")).toHaveLength(16);
    expect(shownEntries("member").map((e) => e.label)).not.toContain("Members");
    expect(shownEntries(undefined).map((e) => e.label)).not.toContain("Members");
  });

  it("narrows by name or group across the groups, and gives nothing for no match", () => {
    const all = shownEntries("owner");
    expect(searchEntries(all, "").map((g) => g.group)).toEqual([...SETTINGS_GROUPS]);
    const rules = searchEntries(all, " RUL ");
    expect(rules).toHaveLength(1);
    expect(rules[0]?.entries.map((e) => e.label)).toEqual(["Rules"]);
    expect(searchEntries(all, "insight")[0]?.entries.map((e) => e.label)).toEqual([
      "Analytics",
      "Retro",
      "Lessons",
    ]);
    expect(searchEntries(all, "zzz")).toEqual([]);
  });

  it("knows the addresses of Settings, which stand at the root", () => {
    for (const path of [
      "/profile",
      "/notifications",
      "/repositories",
      "/appearance",
      "/settings",
    ]) {
      expect(inSettingsArea(path), path).toBe(true);
    }
    expect(inSettingsArea("/inbox")).toBe(false);
    expect(inSettingsArea("/projects/3")).toBe(false);
  });
});
