/**
 * What the Rules page offers the shared filter bar (`ui/filters`): the scope of a rule (the built-in patterns, the
 * server's rules in prose or the rules a project adds) and the project, and which of the page's boxes each choice
 * leaves. Naming a project leaves that project's box alone, since the patterns and the server's rules are not its own.
 */
import type { RulesView } from "../../api/types/rules";
import type { FilterField } from "../../ui/filters/model";

/** The keys of the page's filters in the address, in the order the bar offers them. */
export const FILTER_KEYS = ["scope", "project"] as const;

export const SCOPES: Array<[string, string]> = [
  ["patterns", "Built-in patterns"],
  ["server", "Server rules"],
  ["project", "Project rules"],
];

/** The fields the page is filtered by; a project the page no longer knows still shows by its own number. */
export function filterFields(view: RulesView): FilterField[] {
  return [
    { key: "scope", label: "Scope", options: SCOPES },
    {
      key: "project",
      label: "Project",
      options: view.repositories.map((each): [string, string] => [
        String(each.repository_id),
        each.repository,
      ]),
    },
  ];
}

/** Which boxes of the page the filters leave. */
export function visible(values: Readonly<Record<string, string>>): {
  patterns: boolean;
  server: boolean;
  project: (repository: number) => boolean;
} {
  const { scope = "", project = "" } = values;
  return {
    patterns: project === "" && (scope === "" || scope === "patterns"),
    server: project === "" && (scope === "" || scope === "server"),
    project: (repository) =>
      (scope === "" || scope === "project") && (project === "" || project === String(repository)),
  };
}
