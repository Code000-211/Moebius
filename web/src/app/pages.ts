/**
 * The pages of the app, in groups. The sidebar draws the groups "", "Projects" (its one page, the project list),
 * "Workspace" and "More"; the pages of "Other", "Build", "Improve" and "Setup" are reached from Settings, More or
 * the command bar. Pages that are not built yet
 * show a placeholder; a page that is built replaces its entry's element in `routes.tsx`.
 */
import {
  ArrowsClockwiseIcon,
  BookOpenIcon,
  CalendarDotsIcon,
  ChartLineUpIcon,
  ChatCircleIcon,
  ClockIcon,
  CpuIcon,
  DotsThreeIcon,
  FadersHorizontalIcon,
  FlagIcon,
  FlaskIcon,
  FolderIcon,
  GaugeIcon,
  GearIcon,
  GraphIcon,
  type Icon,
  KeyboardIcon,
  KeyIcon,
  ListChecksIcon,
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
  TrayArrowDownIcon,
  TrayIcon,
  TreeStructureIcon,
  UsersIcon,
} from "@phosphor-icons/react";

/** The sidebar group a page is in; the first group has no name. */
export type Group =
  | ""
  | "Projects"
  | "Workspace"
  | "More"
  | "Other"
  | "Build"
  | "Improve"
  | "Setup";

export interface PageDef {
  /** The path, without the leading slash. */
  path: string;
  title: string;
  group: Group;
  icon: Icon;
  /** Only an owner is shown the page: the sidebar and the command bar leave it out for anyone else. */
  ownerOnly?: boolean;
}

export const PAGES: readonly PageDef[] = [
  { path: "inbox", title: "Inbox", group: "", icon: TrayIcon },
  { path: "my-issues", title: "My issues", group: "", icon: RadioButtonIcon },
  { path: "chat", title: "Chat", group: "Other", icon: ChatCircleIcon },
  { path: "fleet", title: "Fleet", group: "", icon: PulseIcon },
  { path: "automations", title: "Automations", group: "Other", icon: ClockIcon },
  { path: "workflows", title: "Workflows", group: "Other", icon: TreeStructureIcon },
  { path: "projects", title: "Projects", group: "Projects", icon: FolderIcon },
  { path: "milestones", title: "Milestones", group: "Other", icon: FlagIcon },
  { path: "wiki", title: "Wiki", group: "Workspace", icon: BookOpenIcon },
  { path: "cycles", title: "Cycles", group: "Workspace", icon: CalendarDotsIcon },
  { path: "issues", title: "Issues", group: "Other", icon: ListChecksIcon },
  { path: "views", title: "Views", group: "Workspace", icon: StackIcon },
  { path: "analytics", title: "Analytics", group: "Other", icon: ChartLineUpIcon },
  { path: "intake", title: "Intake", group: "Other", icon: TrayArrowDownIcon },
  // Agents, Skills, MCP servers and Rules are held by Settings; the command bar still lists them under "Build".
  { path: "agents", title: "Agents", group: "Build", icon: RobotIcon },
  { path: "skills", title: "Skills", group: "Build", icon: SparkleIcon },
  { path: "mcp-servers", title: "MCP servers", group: "Build", icon: PlugsConnectedIcon },
  { path: "rules", title: "Rules", group: "Build", icon: ScalesIcon },
  { path: "retro", title: "Retro", group: "Improve", icon: ArrowsClockwiseIcon },
  { path: "experiments", title: "Experiments", group: "Improve", icon: FlaskIcon },
  { path: "decision-model", title: "Decisions", group: "Improve", icon: GraphIcon },
  { path: "usage", title: "Usage", group: "Improve", icon: GaugeIcon },
  { path: "shortcuts", title: "Keyboard shortcuts", group: "Setup", icon: KeyboardIcon },
  { path: "start", title: "Get started", group: "Setup", icon: RocketLaunchIcon },
  { path: "models", title: "Models", group: "Setup", icon: CpuIcon },
  { path: "plugins", title: "Plugins", group: "Setup", icon: PlugIcon },
  // The settings plugins ask of the server; those they ask of a repository are on the repository's page.
  { path: "plugin-settings", title: "Plugin settings", group: "Setup", icon: FadersHorizontalIcon },
  { path: "terminal", title: "Terminal", group: "Setup", icon: TerminalWindowIcon },
  { path: "ports", title: "Ports", group: "Setup", icon: PlugsConnectedIcon },
  { path: "settings", title: "Settings", group: "Setup", icon: GearIcon },
  { path: "people", title: "People", group: "Setup", icon: UsersIcon, ownerOnly: true },
  { path: "kinds", title: "Kinds of issues", group: "Setup", icon: ShapesIcon, ownerOnly: true },
  { path: "tokens", title: "My tokens", group: "Setup", icon: KeyIcon },
  // The list of the pages with no other home; the sidebar draws its own link to it, so it has a group of its own.
  { path: "more", title: "More", group: "More", icon: DotsThreeIcon },
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
  "people",
  "plugins",
  "ports",
  "shortcuts",
  "rules",
  "skills",
  "terminal",
  "tokens",
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

/** Whether a person of `role` is shown `page`; a page nobody has a role for yet is shown. */
export function shownTo(page: PageDef, role: string | undefined): boolean {
  return !page.ownerOnly || role === "owner";
}

export function pageByPath(path: string): PageDef | undefined {
  return PAGES.find((page) => page.path === path);
}

/**
 * The path of the board of a project. A project with several repositories has a board for each; `repositoryId`
 * (`?repository=`) names the one to show, and without it the board is the project's first repository's.
 */
export function boardPath(projectId: number, repositoryId?: number): string {
  const base = `/projects/${projectId}`;
  return repositoryId === undefined ? base : `${base}?repository=${repositoryId}`;
}

/** The tabs of a project after its Board, in the order the header shows them. */
export const PROJECT_TABS = ["workflows", "cycles", "milestones", "intake"] as const;
export type ProjectTab = (typeof PROJECT_TABS)[number];

/** The path of a tab of a project: its board for `board`, else `/projects/:id/<tab>`. */
export function projectTabPath(projectId: number, tab: "board" | ProjectTab): string {
  return tab === "board" ? boardPath(projectId) : `${boardPath(projectId)}/${tab}`;
}

/** The path of the board of a repository: at the project that is its home, naming the repository. */
export function repositoryBoardPath(
  repository: { id: number; home_project_id: number },
  cardId?: number,
): string {
  return cardId === undefined
    ? boardPath(repository.home_project_id, repository.id)
    : cardPath(repository.home_project_id, cardId, repository.id);
}

/** The path of the board of a project with one card open in the dock. */
export function cardPath(projectId: number, cardId: number, repositoryId?: number): string {
  const repository = repositoryId === undefined ? "" : `repository=${repositoryId}&`;
  return `${boardPath(projectId)}?${repository}card=${cardId}`;
}

/** The path of the page of a repository under the project that holds it. */
export function repositoryPath(projectId: number, repositoryId: number): string {
  return `/projects/${projectId}/repositories/${repositoryId}`;
}

/** The path of the editor of the script a column of a project runs. */
export function scriptPath(projectId: number, column: string, repositoryId?: number): string {
  const query = repositoryId === undefined ? "" : `?repository=${repositoryId}`;
  return `/projects/${projectId}/settings/columns/${encodeURIComponent(column)}${query}`;
}

/** The path of a project's settings, at `tab` when one is named. */
export function settingsPath(projectId: number, tab?: string): string {
  return `/projects/${projectId}/settings${tab && tab !== "general" ? `/${tab}` : ""}`;
}
