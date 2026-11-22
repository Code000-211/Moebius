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
  BookOpenIcon,
  ChartLineUpIcon,
  ChatCircleIcon,
  ClockIcon,
  CpuIcon,
  FlaskIcon,
  FolderIcon,
  GaugeIcon,
  GearIcon,
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
export const SETTINGS_GROUPS = ["You", "Server"] as const;
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
    path: "shortcuts",
    label: "Keyboard shortcuts",
    icon: KeyboardIcon,
    shownTo: everyone,
    settings: { group: "You" },
    words: "keys",
  },
  {
    path: "tokens",
    label: "My tokens",
    icon: KeyIcon,
    shownTo: everyone,
    settings: { group: "You" },
    words: "api access",
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
    path: "agents/skills",
    label: "Skills",
    icon: SparkleIcon,
    shownTo: owner,
    words: "agents",
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
    path: "insights",
    label: "Insights",
    icon: ChartLineUpIcon,
    shownTo: owner,
    sidebar: "Insight",
    words: "analytics usage charts cost tokens spend",
  },
  {
    path: "insights/usage",
    label: "Usage",
    icon: GaugeIcon,
    shownTo: owner,
    words: "tokens spend insights",
  },
  {
    path: "improve",
    label: "Improve",
    icon: ArrowsClockwiseIcon,
    shownTo: owner,
    sidebar: "Insight",
    words: "retro retrospective lessons experiments decisions export training",
  },
  {
    path: "improve/lessons",
    label: "Lessons",
    icon: GraduationCapIcon,
    shownTo: owner,
    words: "improve learned",
  },
  {
    path: "improve/experiments",
    label: "Experiments",
    icon: FlaskIcon,
    shownTo: owner,
    words: "improve research",
  },
  {
    path: "improve/decisions",
    label: "Decisions",
    icon: GraphIcon,
    shownTo: owner,
    words: "improve decision model export training data",
  },
  {
    path: "settings",
    label: "Settings",
    icon: GearIcon,
    shownTo: owner,
    settings: { group: "Server", label: "General" },
    words: "preferences",
  },
  {
    path: "models",
    label: "Models",
    icon: CpuIcon,
    shownTo: owner,
    settings: { group: "Server" },
    words: "providers",
  },
  {
    path: "plugins",
    label: "Plugins",
    icon: PlugIcon,
    shownTo: owner,
    settings: { group: "Server" },
    words: "extensions settings",
  },
  {
    path: "kinds",
    label: "Kinds of issues",
    icon: ShapesIcon,
    shownTo: owner,
    settings: { group: "Server" },
    words: "types",
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

/** An entry of the Settings navigation: a row with the name, group and icon it has there. */
export interface SettingsEntry {
  group: SettingsGroup;
  label: string;
  path: string;
  icon: Icon;
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
            icon: entry.icon,
          },
        ]
      : [],
  );
}

/** The groups of `entries` that hold any, in the groups' order. */
export function groupEntries(
  entries: readonly SettingsEntry[],
): Array<{ group: SettingsGroup; entries: SettingsEntry[] }> {
  return SETTINGS_GROUPS.map((group) => ({
    group,
    entries: entries.filter((e) => e.group === group),
  })).filter((g) => g.entries.length > 0);
}

/**
 * Whether `pathname` is a page the sidebar chooses Settings on: an entry of its navigation,
 * unless the sidebar has a row of its own for the page.
 */
export function inSettingsArea(pathname: string): boolean {
  const first = pathname.replace(/^\/+/, "").split("/")[0] ?? "";
  const entry = entryByPath(first);
  return entry !== undefined && entry.sidebar === undefined && !!entry.settings;
}
