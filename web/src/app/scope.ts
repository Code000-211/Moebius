/**
 * The project and the agent filters of the pages that read figures the server can narrow to the runs of one repository
 * and one agent (Usage and Decisions): the fields the shared filter bar offers, what the address holds of them, and the
 * `Scope` a read takes. A project is a repository here (the address keeps its id under `project`); the agents are the
 * server's, read from `GET /v1/agents`.
 */
import { useQuery } from "@tanstack/react-query";
import { snapshot as readSnapshot } from "../api/calls/board";
import { agents as readAgents } from "../api/calls/chat";
import type { Scope } from "../api/calls/usage";
import { keys } from "../api/keys";
import { type FilterField, readFilters } from "../ui/filters/model";
import { useQueryParams } from "../ui/filters/useQueryParams";

/** The keys of the two filters in the address, in the order the bar offers them. */
export const SCOPE_KEYS = ["project", "agent"] as const;

/** A repository id as the address holds it; null for text that is not one. */
export function repositoryOf(value: string): number | null {
  return /^\d+$/.test(value) ? Number(value) : null;
}

export interface ScopeFilters {
  fields: FilterField[];
  values: Record<string, string>;
  scope: Scope;
  set: (key: string, value: string) => void;
  clear: () => void;
}

/** The two filters of the page, read from the address and from the repositories and agents the server has. */
export function useScopeFilters(): ScopeFilters {
  const [params, setQuery] = useQueryParams();
  const boards = useQuery({ queryKey: keys.snapshot, queryFn: () => readSnapshot() });
  const agents = useQuery({ queryKey: keys.agents, queryFn: readAgents });
  const fields: FilterField[] = [
    {
      key: "project",
      label: "Project",
      options: (boards.data?.boards ?? []).map((board): [string, string] => [
        String(board.repository.id),
        board.repository.name,
      ]),
    },
    {
      key: "agent",
      label: "Agent",
      options: [...(agents.data ?? [])]
        .map((agent) => agent.name)
        .sort((a, b) => a.localeCompare(b))
        .map((name): [string, string] => [name, name]),
    },
  ];
  const values = readFilters(params, fields);
  return {
    fields,
    values,
    scope: { repository: repositoryOf(values.project ?? ""), agent: values.agent ?? "" },
    set: (key, value) => setQuery({ [key]: value }),
    clear: () => setQuery(Object.fromEntries(SCOPE_KEYS.map((key) => [key, ""]))),
  };
}
