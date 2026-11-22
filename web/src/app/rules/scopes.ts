/**
 * The scopes of the Rules page: the sets of rules its sub-navigation chooses between (the built-in patterns, the
 * server's rules, one repository's rules) and the list of what refused calls this week. The chosen one is the `scope`
 * parameter of the address: `builtin`, `server`, `refusals` or `repository-<id>`. A link that names a scope the page
 * does not know (a repository gone, or the `?scope=patterns` and `?project=` of the earlier page) still lands on a
 * scope that exists.
 */
import type { RulesView } from "../../api/types/rules";

export type Scope =
  | { kind: "builtin" }
  | { kind: "server" }
  | { kind: "repository"; id: number }
  | { kind: "refusals" };

/** The address value of a scope. */
export function scopeKey(scope: Scope): string {
  return scope.kind === "repository" ? `repository-${scope.id}` : scope.kind;
}

/** The scope of an address value, or null when it names none of this view's. */
function parse(value: string, view: RulesView): Scope | null {
  if (value === "builtin" || value === "patterns") return { kind: "builtin" };
  if (value === "server") return { kind: "server" };
  if (value === "refusals") return { kind: "refusals" };
  const id = /^repository-(\d+)$/.exec(value)?.[1] ?? value;
  const known = view.repositories.find((each) => String(each.repository_id) === id);
  return known && /^(repository-)?\d+$/.test(value) ? { kind: "repository", id: Number(id) } : null;
}

/** The scope the address chooses; without one, the server's if it has rules, else the built-in patterns. */
export function chosenScope(params: URLSearchParams, view: RulesView): Scope {
  const named = params.get("scope") ?? params.get("project") ?? "";
  return (
    parse(named, view) ?? (view.prose.rules.length > 0 ? { kind: "server" } : { kind: "builtin" })
  );
}

/** How a scope is named in a sentence and in the navigation. */
export function scopeName(scope: Scope, view: RulesView): string {
  switch (scope.kind) {
    case "builtin":
      return "Built-in";
    case "server":
      return "Server";
    case "refusals":
      return "Refusals";
    case "repository":
      return view.repositories.find((each) => each.repository_id === scope.id)?.repository ?? "";
  }
}
