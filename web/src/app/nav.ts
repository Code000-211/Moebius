/**
 * The one table of the app's places. The sidebar, the command bar and the navigation of Settings all read it, so a
 * place is written once: its path, the name it shows, whether only an owner is shown it, the words the command bar
 * finds it by, and where it stands. `sidebar` is the group of the sidebar the place is a row of; `settings` is the
 * group of the Settings navigation it is an entry of, with its own name there when that differs (the page of
 * `/settings` is "General" inside Settings, where "Settings" would name the whole set). A row with neither is reached
 * from the command bar and from the pages that link to it. A later step of the restructuring (`docs/adr/0024-…`) adds
 * a row here or deletes one, and nothing else lists a page.
 *
 * The order of the rows is the order of every list drawn from them. A path is in the table once.
 */
import {
  ArrowsClockwiseIcon,
  BellIcon,
  BookOpenIcon,
  BracketsCurlyIcon,
  ChartLineUpIcon,
  ChatCircleIcon,
  ClockIcon,
  CpuIcon,
  DownloadSimpleIcon,
  FadersHorizontalIcon,
  FlaskIcon,
  FolderIcon,
  GaugeIcon,
  GearIcon,
  GitBranchIcon,
  GraduationCapIcon,
  GraphIcon,
  type Icon,
  KeyboardIcon,
  KeyIcon,
  ListChecksIcon,
  PaletteIcon,
  PlugIcon,
  PlugsConnectedIcon,
  PulseIcon,
  RadioButtonIcon,
  RobotIcon,
  RocketLaunchIcon,
  ScalesIcon,
  ShapesIcon,
  SparkleIcon,
  StackIcon,
  TerminalWindowIcon,
  TrayIcon,
  TreeStructureIcon,
  UserCircleIcon,
  UsersIcon,
} from "@phosphor-icons/react";

/** The groups of the sidebar, in order. The first has no name; "Projects" is the group of the project rows. */
export const SIDEBAR_GROUPS = ["", "Projects", "Workspace", "Build", "Insight"] as const;
export type SidebarGroup = (typeof SIDEBAR_GROUPS)[number];

/** The groups of the Settings navigation, in order. */
export const SETTINGS_GROUPS = ["You", "Agents", "Automation", "Insight", "Server"] as const;
export type SettingsGroup = (typeof SETTINGS_GROUPS)[number];

export interface NavEntry {
  /** The path, without the leading slash. */
  path: string;
  label: string;
  icon: Icon;
  /** Who is shown the place: everyone, or only an owner. The sidebar, the command bar and Settings leave it out for anyone else. */
  shownTo: "everyone" | "owner";
  /** The group of the sidebar the place is a row of. */
  sidebar?: SidebarGroup;
  /** The group of the Settings navigation the place is an entry of, and the name it carries there. */
  settings?: { group: SettingsGroup; label?: string };
  /** A place Settings reaches without listing it in its navigation: the sidebar chooses Settings on it. */
  inSettings?: true;
  /** What the command bar finds the place by, besides its name and group. */
  words?: string;
}

const everyone = "everyone";
const owner = "owner";

export const NAV: readonly NavEntry[] = [
  {
    path: "inbox",
    label: "Inbox",
    icon: TrayIcon,
    shownTo: everyone,
    sidebar: "",
    words: "notifications",
  },
  {
    path: "my-issues",
    label: "My issues",
    icon: RadioButtonIcon,
    shownTo: everyone,
    sidebar: "",
    words: "assigned mine",
  },
  {
    path: "fleet",
    label: "Fleet",
    icon: PulseIcon,
    shownTo: everyone,
    sidebar: "",
    words: "runs runners ports agents running ssh machines forwarding",
  },
  {
    path: "chat",
    label: "Chat",
    icon: ChatCircleIcon,
    shownTo: everyone,
    sidebar: "",
    words: "talk conversation",
  },
  {
    path: "projects",
    label: "Projects",
    icon: FolderIcon,
    shownTo: everyone,
    sidebar: "Projects",
    words: "boards repositories",
  },
  {
    path: "wiki",
    label: "Wiki",
    icon: BookOpenIcon,
    shownTo: everyone,
    sidebar: "Workspace",
    words: "pages notes",
  },
  {
    path: "views",
    label: "Views",
    icon: StackIcon,
    shownTo: everyone,
    sidebar: "Workspace",
    words: "filters saved",
  },
  {
    path: "views/all-issues",
    label: "All issues",
    icon: ListChecksIcon,
    shownTo: everyone,
    words: "all issues list",
  },
  {
    path: "profile",
    label: "Profile",
    icon: UserCircleIcon,
    shownTo: everyone,
    settings: { group: "You" },
    words: "name account",
  },
  {
    path: "appearance",
    label: "Appearance",
    icon: PaletteIcon,
    shownTo: everyone,
    settings: { group: "You" },
    words: "theme dark light",
  },
  {
    path: "notifications",
    label: "Notifications",
    icon: BellIcon,
    shownTo: everyone,
    settings: { group: "You" },
    words: "alerts",
  },
  {
    path: "shortcuts",
    label: "Keyboard shortcuts",
    icon: KeyboardIcon,
    shownTo: everyone,
    settings: { group: "You", label: "Keyboard" },
    words: "keys",
  },
  {
    path: "tokens",
    label: "My tokens",
    icon: KeyIcon,
    shownTo: everyone,
    words: "api access",
    inSettings: true,
  },
  {
    path: "agents",
    label: "Agents",
    icon: RobotIcon,
    shownTo: owner,
    sidebar: "Build",
    words: "coder reviewer skills mcp servers tools",
  },
  {
    path: "models",
    label: "Models",
    icon: CpuIcon,
    shownTo: everyone,
    settings: { group: "Agents" },
    words: "providers",
  },
  {
    path: "agents/skills",
    label: "Skills",
    icon: SparkleIcon,
    shownTo: owner,
    words: "agents",
  },
  {
    path: "plugins",
    label: "Plugins",
    icon: PlugIcon,
    shownTo: everyone,
    settings: { group: "Agents" },
    words: "extensions",
  },
  {
    path: "agents/mcp-servers",
    label: "MCP servers",
    icon: PlugsConnectedIcon,
    shownTo: owner,
    words: "tools agents",
  },
  {
    path: "rules",
    label: "Rules",
    icon: ScalesIcon,
    shownTo: owner,
    sidebar: "Build",
    words: "permissions guard patterns refusals",
  },
  {
    path: "workflows",
    label: "Workflows",
    icon: TreeStructureIcon,
    shownTo: everyone,
    words: "canvas schedule",
  },
  {
    path: "automations",
    label: "Automations",
    icon: ClockIcon,
    shownTo: everyone,
    words: "scheduled chat",
  },
  {
    path: "column-scripts",
    label: "Column scripts",
    icon: BracketsCurlyIcon,
    shownTo: everyone,
    settings: { group: "Automation" },
    words: "luau",
  },
  {
    path: "schedules",
    label: "Schedules",
    icon: ClockIcon,
    shownTo: everyone,
    settings: { group: "Automation" },
    words: "cron",
  },
  {
    path: "analytics",
    label: "Insights",
    icon: ChartLineUpIcon,
    shownTo: owner,
    sidebar: "Insight",
    settings: { group: "Insight", label: "Analytics" },
    words: "analytics charts cost",
  },
  {
    path: "retro",
    label: "Improve",
    icon: ArrowsClockwiseIcon,
    shownTo: owner,
    sidebar: "Insight",
    settings: { group: "Insight", label: "Retro" },
    words: "retro retrospective",
  },
  {
    path: "lessons",
    label: "Lessons",
    icon: GraduationCapIcon,
    shownTo: everyone,
    settings: { group: "Insight" },
  },
  {
    path: "export",
    label: "Export",
    icon: DownloadSimpleIcon,
    shownTo: everyone,
    settings: { group: "Insight" },
    words: "training data",
  },
  { path: "experiments", label: "Experiments", icon: FlaskIcon, shownTo: everyone },
  {
    path: "decision-model",
    label: "Decisions",
    icon: GraphIcon,
    shownTo: everyone,
    words: "decision model",
  },
  {
    path: "usage",
    label: "Usage",
    icon: GaugeIcon,
    shownTo: everyone,
    words: "tokens spend",
    inSettings: true,
  },
  {
    path: "settings",
    label: "Settings",
    icon: GearIcon,
    shownTo: everyone,
    settings: { group: "Server", label: "General" },
    words: "preferences",
  },
  {
    path: "repositories",
    label: "Repositories",
    icon: GitBranchIcon,
    shownTo: everyone,
    settings: { group: "Server" },
  },
  {
    path: "people",
    label: "Members",
    icon: UsersIcon,
    shownTo: owner,
    settings: { group: "Server" },
    words: "people users",
  },
  {
    path: "kinds",
    label: "Kinds of issues",
    icon: ShapesIcon,
    shownTo: owner,
    words: "types",
    inSettings: true,
  },
  {
    path: "plugin-settings",
    label: "Plugin settings",
    icon: FadersHorizontalIcon,
    shownTo: everyone,
    inSettings: true,
  },
  {
    path: "terminal",
    label: "Terminal",
    icon: TerminalWindowIcon,
    shownTo: everyone,
    words: "shell console",
  },
  {
    path: "start",
    label: "Get started",
    icon: RocketLaunchIcon,
    shownTo: everyone,
    words: "setup steps",
  },
];

export const DEFAULT_PAGE = "inbox";

/** Whether a person of `role` is shown `entry`; a person with no role yet is shown what everyone is. */
export function shownTo(entry: NavEntry, role: string | undefined): boolean {
  return entry.shownTo === "everyone" || role === "owner";
}

export function entryByPath(path: string): NavEntry | undefined {
  return NAV.find((entry) => entry.path === path);
}

/** The rows of a group of the sidebar a person of `role` is shown, in the table's order. */
export function sidebarRows(group: SidebarGroup, role: string | undefined): NavEntry[] {
  return NAV.filter((entry) => entry.sidebar === group && shownTo(entry, role));
}

/** An entry of the Settings navigation: a row with the name and group it has there. */
export interface SettingsEntry {
  group: SettingsGroup;
  label: string;
  path: string;
}

/** The entries of the Settings navigation a person of `role` is shown, in the table's order. */
export function settingsEntries(role: string | undefined): SettingsEntry[] {
  return NAV.flatMap((entry) =>
    entry.settings && shownTo(entry, role)
      ? [
          {
            group: entry.settings.group,
            label: entry.settings.label ?? entry.label,
            path: entry.path,
          },
        ]
      : [],
  );
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

/**
 * Whether `pathname` is a page the sidebar chooses Settings on: an entry of its navigation, or a place it reaches,
 * unless the sidebar has a row of its own for the page (Agents and Rules, and for now Insights and Improve, are entries of both).
 */
export function inSettingsArea(pathname: string): boolean {
  const first = pathname.replace(/^\/+/, "").split("/")[0] ?? "";
  const entry = entryByPath(first);
  return (
    entry !== undefined && entry.sidebar === undefined && (!!entry.settings || !!entry.inSettings)
  );
}
