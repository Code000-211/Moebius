/**
 * The legacy table: every address the app used to answer that now lives elsewhere, as the path pattern it had and
 * what it leads to. `routes.tsx` turns each line into a route that replaces the address (so Back does not return to
 * it) and keeps what was in the query, such as `?card=`. Later restructuring steps add a line here for each address
 * they remove, and nothing else in `routes.tsx`.
 *
 * A resolver gets the pattern's params, the projects the person sees and the old query, and answers the new address
 * (query included) or null when nothing is there any more, which shows the page for an address that is none.
 * This is the one place that may name `/board/`: components use the path helpers of `pages.ts`.
 */
import type { Project } from "../../api/types/projects";
import { boardPath, cyclePath, projectTabPath, repositoryPath, scriptPath } from "../pages";
import { holdsRepository, homeProjectOf, projectForOldAddress } from "../projects/model";

export interface LegacyRoute {
  /** The pattern of the old path, without the leading slash, in the syntax of the router. */
  path: string;
  resolve: (
    params: Record<string, string | undefined>,
    projects: Project[],
    search: URLSearchParams,
  ) => string | null;
}

/** `path` with `first` and then every pair of `search` (except those named in `drop`) as its query. */
function withQuery(
  path: string,
  first: Record<string, string>,
  search: URLSearchParams,
  drop: string[] = [],
): string {
  const query = new URLSearchParams(first);
  for (const [key, value] of search)
    if (!drop.includes(key) && !query.has(key)) query.set(key, value);
  const text = query.toString();
  return text === "" ? path : `${path}?${text}`;
}

/** The id an address names, or null when it is not a whole number. */
function idOf(text: string | undefined): number | null {
  return text !== undefined && /^\d+$/.test(text) ? Number(text) : null;
}

/**
 * Where `/projects/:id` leads when no project has that id: before the repository page moved under its project, it
 * was at `/projects/<repository id>`. A repository that some project holds is found at its home project; anything
 * else is no address (null). A project always wins over a repository of the same number.
 */
export function oldRepositoryPage(
  id: string | undefined,
  projects: Project[],
  search: URLSearchParams,
): string | null {
  const repository = idOf(id);
  if (repository === null) return null;
  const home = homeProjectOf(projects, repository);
  return home ? withQuery(repositoryPath(home.id, repository), {}, search) : null;
}

/**
 * Where the old project-less Cycles, Milestones and Intake addresses lead: the tab of the project named by `?project=`,
 * else of the one opened last or the only one, else the list of projects. `drop` is what the query no longer carries.
 */
function oldTab(
  tab: "cycles" | "milestones" | "intake",
  projects: Project[],
  search: URLSearchParams,
  cycle?: number | null,
): string {
  const project = projectForOldAddress(projects, search);
  if (!project) return "/projects";
  const path = cycle ? cyclePath(project.id, cycle) : projectTabPath(project.id, tab);
  return withQuery(path, {}, search, ["project", "cycle"]);
}

export const LEGACY_ROUTES: readonly LegacyRoute[] = [
  {
    // More listed pages that now have a place of their own; the list itself is gone, and the Inbox is home.
    path: "more",
    resolve: () => "/inbox",
  },
  {
    // The board of a repository is the board of its home project, at that repository.
    path: "board/:repoId",
    resolve: ({ repoId }, projects, search) => {
      const repository = idOf(repoId);
      const home = repository === null ? undefined : homeProjectOf(projects, repository);
      if (repository === null || !home) return null;
      return withQuery(boardPath(home.id), { repository: String(repository) }, search);
    },
  },
  {
    // The script of a column: of the project the card named (`?project=`) when it still holds the repository.
    path: "board/:repoId/scripts/:column",
    resolve: ({ repoId, column }, projects, search) => {
      const repository = idOf(repoId);
      if (repository === null || column === undefined) return null;
      const named = projects.find((each) => String(each.id) === search.get("project"));
      const project =
        named && holdsRepository(named, repository) ? named : homeProjectOf(projects, repository);
      if (!project) return null;
      return withQuery(scriptPath(project.id, column, repository), {}, search, [
        "project",
        "repository",
      ]);
    },
  },
  {
    // The cycle that was open (`?cycle=`) is the cycle page of the project.
    path: "cycles",
    resolve: (_params, projects, search) =>
      oldTab("cycles", projects, search, idOf(search.get("cycle") ?? undefined)),
  },
  {
    path: "cycles/:cycleId",
    resolve: ({ cycleId }, projects, search) => oldTab("cycles", projects, search, idOf(cycleId)),
  },
  {
    path: "milestones",
    resolve: (_params, projects, search) => oldTab("milestones", projects, search),
  },
  { path: "intake", resolve: (_params, projects, search) => oldTab("intake", projects, search) },
  {
    // Skills and MCP servers are tabs of Agents; `?skill=` and `?server=` open the same window there.
    path: "skills",
    resolve: (_params, _projects, search) => withQuery("/agents/skills", {}, search),
  },
  {
    path: "mcp-servers",
    resolve: (_params, _projects, search) => withQuery("/agents/mcp-servers", {}, search),
  },
  {
    // Agents and Rules were Settings pages first: `/settings/agents`, `/settings/skills` and the like.
    path: "settings/agents",
    resolve: (_params, _projects, search) => withQuery("/agents", {}, search),
  },
  {
    path: "settings/skills",
    resolve: (_params, _projects, search) => withQuery("/agents/skills", {}, search),
  },
  {
    path: "settings/mcp-servers",
    resolve: (_params, _projects, search) => withQuery("/agents/mcp-servers", {}, search),
  },
  {
    path: "settings/rules",
    resolve: (_params, _projects, search) => withQuery("/rules", {}, search),
  },
];
