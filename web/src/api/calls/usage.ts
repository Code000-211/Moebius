/** The call about what the model calls used. */
import { get } from "../http";
import type { UsageReport } from "../types/usage";

/** What narrows a read to the runs of a repository and of an agent; empty for no narrowing. */
export interface Scope {
  repository?: number | null;
  agent?: string;
}

/** The parameters that narrow a read to a scope, left out when not set. */
export function scopeQuery(scope: Scope): URLSearchParams {
  const query = new URLSearchParams();
  if (scope.repository != null) query.set("repository", String(scope.repository));
  if (scope.agent) query.set("agent", scope.agent);
  return query;
}

/** The usage of the last `days` local days of the server, today included, of the runs in `scope`. */
export function usage(days: number, scope: Scope = {}): Promise<UsageReport> {
  const query = scopeQuery(scope);
  query.set("days", String(days));
  return get<UsageReport>(`/v1/usage?${query}`);
}
