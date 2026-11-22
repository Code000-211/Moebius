/**
 * The addresses of the app's pages that carry an id: a project's board and its tabs, a card, a repository under its
 * project, the editor of a column's script and a project's settings. The list of places itself, with their names and
 * who is shown them, is the table in `nav.ts`.
 */

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

/** The tabs of a workflow's page, in the order its header shows them. */
export const WORKFLOW_TABS = ["overview", "canvas", "runs", "file"] as const;
export type WorkflowTab = (typeof WORKFLOW_TABS)[number];

/** The path of a workflow's page, under the Workflows tab of its project: its Overview, or the tab asked for. */
export function workflowPath(
  projectId: number,
  name: string,
  tab: WorkflowTab = "overview",
): string {
  const page = `${projectTabPath(projectId, "workflows")}/${encodeURIComponent(name)}`;
  return tab === "overview" ? page : `${page}/${tab}`;
}

/** The path of the Canvas tab of a workflow. */
export function workflowCanvasPath(projectId: number, name: string): string {
  return workflowPath(projectId, name, "canvas");
}

/** The path of one run of a workflow; a run is addressed by its id, and its page links back to its project. */
export function workflowRunPath(runId: number): string {
  return `/workflow-runs/${runId}`;
}

/** The path of the built-in view "All issues" of the Views page (what `/issues` was). */
export const ALL_ISSUES_PATH = "/views/all-issues";

/** The path of one cycle's page, under the Cycles tab of its project. */
export function cyclePath(projectId: number, cycleId: number): string {
  return `${projectTabPath(projectId, "cycles")}/${cycleId}`;
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

/** The path of an agent's page, at `tab` when one is named (Settings is the first and needs no mention). */
export function agentPath(name: string, tab?: string): string {
  const base = `/agents/${encodeURIComponent(name)}`;
  return tab && tab !== "settings" ? `${base}?tab=${tab}` : base;
}

/** The page of a new agent, which is named there (`?blank=1`, or `?from=<agent>` to start from a copy). */
export const NEW_AGENT_PATH = "/agents/new";

/** What no agent may be called: the addresses under `/agents` that are pages of their own. */
export const RESERVED_AGENT_NAMES = ["new", "skills", "mcp-servers"];
