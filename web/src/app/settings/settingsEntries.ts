/**
 * The entries of the Settings navigation, as data: the complete decided list of twenty (`docs/design/web-redesign/DECISIONS.md`),
 * in five groups. An entry has the group it stands in, the name it shows, the path of its page, whether only an
 * owner is shown it, and the page it opens once one is registered. The navigation (`SettingsNav`) and the routes
 * (`routes.tsx`) both read this list, so a later issue that builds a page adds it by writing its `page` on one line
 * here. The paths are those the pages always had, so every address keeps working.
 */
import { type ComponentType, lazy } from "react";

export const SETTINGS_GROUPS = ["You", "Agents", "Automation", "Insight", "Server"] as const;
export type SettingsGroup = (typeof SETTINGS_GROUPS)[number];

export interface SettingsEntry {
  group: SettingsGroup;
  label: string;
  /** The path, without the leading slash. */
  path: string;
  /** Only an owner is shown the entry. */
  ownerOnly?: boolean;
  /** The page; absent while no page is built for the entry, which then stays out of the navigation. */
  page?: ComponentType;
}

const named = <T extends string>(load: () => Promise<Record<T, ComponentType>>, name: T) =>
  lazy(() => load().then((all) => ({ default: all[name] })));

const Appearance = named(() => import("./AppearancePage"), "AppearancePage");
const General = named(() => import("./SettingsPage"), "SettingsPage");
const Shortcuts = named(() => import("../shortcuts/ShortcutsPage"), "ShortcutsPage");
const Agents = named(() => import("../agents/AgentsPage"), "AgentsPage");
const Models = named(() => import("../models/ModelsPage"), "ModelsPage");
const Skills = named(() => import("../skills/SkillsPage"), "SkillsPage");
const Plugins = named(() => import("../plugins/PluginsPage"), "PluginsPage");
const Rules = named(() => import("../rules/RulesPage"), "RulesPage");
const Workflows = named(() => import("../workflows/WorkflowsPage"), "WorkflowsPage");
const Analytics = named(() => import("../analytics/AnalyticsPage"), "AnalyticsPage");
const Retro = named(() => import("../retro/RetroPage"), "RetroPage");
const Lessons = named(() => import("./LessonsPage"), "LessonsPage");
const People = named(() => import("../people/PeoplePage"), "PeoplePage");

export const SETTINGS_ENTRIES: readonly SettingsEntry[] = [
  { group: "You", label: "Profile", path: "profile" },
  { group: "You", label: "Appearance", path: "appearance", page: Appearance },
  { group: "You", label: "Notifications", path: "notifications" },
  { group: "You", label: "Keyboard", path: "shortcuts", page: Shortcuts },
  { group: "Agents", label: "Agents", path: "agents", page: Agents },
  { group: "Agents", label: "Models", path: "models", page: Models },
  { group: "Agents", label: "Skills", path: "skills", page: Skills },
  { group: "Agents", label: "Plugins", path: "plugins", page: Plugins },
  { group: "Automation", label: "Rules", path: "rules", page: Rules },
  { group: "Automation", label: "Workflows", path: "workflows", page: Workflows },
  { group: "Automation", label: "Column scripts", path: "column-scripts" },
  { group: "Automation", label: "Schedules", path: "schedules" },
  { group: "Insight", label: "Analytics", path: "analytics", page: Analytics },
  { group: "Insight", label: "Retro", path: "retro", page: Retro },
  { group: "Insight", label: "Lessons", path: "lessons", page: Lessons },
  { group: "Insight", label: "Export", path: "export" },
  { group: "Server", label: "General", path: "settings", page: General },
  { group: "Server", label: "Repositories", path: "repositories" },
  { group: "Server", label: "Runners", path: "runners" },
  { group: "Server", label: "Members", path: "people", ownerOnly: true, page: People },
];

/** The entries a person of `role` is shown: those with a page, minus the owner's for anyone else. */
export function shownEntries(role: string | undefined): SettingsEntry[] {
  return SETTINGS_ENTRIES.filter((e) => e.page && (!e.ownerOnly || role === "owner"));
}

/** The groups of `entries` that hold an entry matching `query` (by its name or its group's), in the list's order. */
export function searchEntries(
  entries: readonly SettingsEntry[],
  query: string,
): Array<{ group: SettingsGroup; entries: SettingsEntry[] }> {
  const needle = query.trim().toLowerCase();
  return SETTINGS_GROUPS.map((group) => ({
    group,
    entries: entries.filter(
      (e) =>
        e.group === group &&
        (needle === "" ||
          e.label.toLowerCase().includes(needle) ||
          e.group.toLowerCase().includes(needle)),
    ),
  })).filter((g) => g.entries.length > 0);
}
