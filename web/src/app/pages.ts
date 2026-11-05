/**
 * The pages of the app: the entries of the sidebar, in its groups, as the former desktop app listed them
 * (the sidebar of the former desktop app). Pages that are not built in the web
 * app yet show a placeholder; a page that is built replaces its entry's element in `routes.tsx`.
 */
import {
  ArrowsClockwiseIcon,
  ChatCircleIcon,
  ClockIcon,
  CpuIcon,
  FadersHorizontalIcon,
  FlaskIcon,
  FolderIcon,
  GaugeIcon,
  GearIcon,
  GraphIcon,
  type Icon,
  PlugIcon,
  PlugsConnectedIcon,
  RobotIcon,
  RocketLaunchIcon,
  ScalesIcon,
  SparkleIcon,
  TerminalWindowIcon,
  TrayIcon,
  UsersThreeIcon,
} from "@phosphor-icons/react";

/** The sidebar group a page is in; the first group has no name. */
export type Group = "" | "Projects" | "Build" | "Improve" | "Setup";

export interface PageDef {
  /** The path, without the leading slash. */
  path: string;
  title: string;
  group: Group;
  icon: Icon;
}

export const PAGES: readonly PageDef[] = [
  { path: "inbox", title: "Inbox", group: "", icon: TrayIcon },
  { path: "chat", title: "Chat", group: "", icon: ChatCircleIcon },
  { path: "fleet", title: "Fleet", group: "", icon: UsersThreeIcon },
  { path: "automations", title: "Automations", group: "", icon: ClockIcon },
  { path: "projects", title: "Projects", group: "Projects", icon: FolderIcon },
  { path: "agents", title: "Agents", group: "Build", icon: RobotIcon },
  { path: "skills", title: "Skills", group: "Build", icon: SparkleIcon },
  { path: "mcp-servers", title: "MCP servers", group: "Build", icon: PlugsConnectedIcon },
  { path: "rules", title: "Rules", group: "Build", icon: ScalesIcon },
  { path: "retro", title: "Retro", group: "Improve", icon: ArrowsClockwiseIcon },
  { path: "experiments", title: "Experiments", group: "Improve", icon: FlaskIcon },
  { path: "decision-model", title: "Decisions", group: "Improve", icon: GraphIcon },
  { path: "usage", title: "Usage", group: "Improve", icon: GaugeIcon },
  { path: "start", title: "Get started", group: "Setup", icon: RocketLaunchIcon },
  { path: "models", title: "Models", group: "Setup", icon: CpuIcon },
  { path: "plugins", title: "Plugins", group: "Setup", icon: PlugIcon },
  // Only in the web app: the desktop app has no page for the plugins' settings.
  { path: "plugin-settings", title: "Plugin settings", group: "Setup", icon: FadersHorizontalIcon },
  { path: "terminal", title: "Terminal", group: "Setup", icon: TerminalWindowIcon },
  { path: "ports", title: "Ports", group: "Setup", icon: PlugsConnectedIcon },
  { path: "settings", title: "Settings", group: "Setup", icon: GearIcon },
];

export const DEFAULT_PAGE = "inbox";

/**
 * The pages that belong to Settings, in the order its sidebar lists them: alphabetically by the name each entry
 * shows, because a set of ten places nobody uses every day is looked for rather than remembered. They are pages like
 * any other — the routes do not change — but the everyday sidebar has one entry for the whole set, so that it carries
 * the places a person goes to every day rather than the ones they set up once.
 */
export const SETTINGS_PAGES: readonly PageDef[] = [
  "agents",
  "settings",
  "mcp-servers",
  "models",
  "plugin-settings",
  "plugins",
  "ports",
  "rules",
  "skills",
  "terminal",
  "usage",
]
  .map((path) => PAGES.find((page) => page.path === path))
  .filter((page): page is PageDef => page !== undefined);

/** Whether `path` (without the leading slash) is one of the pages Settings holds. */
export function isSettingsPage(path: string): boolean {
  return SETTINGS_PAGES.some((page) => page.path === path);
}

/** The name the entry of `/settings` carries inside Settings, where "Settings" would name the whole set. */
export function settingsLabel(page: PageDef): string {
  return page.path === "settings" ? "General" : page.title;
}

export function pageByPath(path: string): PageDef | undefined {
  return PAGES.find((page) => page.path === path);
}

/** The path of the board of a repository. */
export function boardPath(repositoryId: number): string {
  return `/board/${repositoryId}`;
}
