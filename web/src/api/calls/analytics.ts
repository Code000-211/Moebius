/** The call about throughput, times, burn-ups and tokens per point. */
import { get } from "../http";
import type { Analytics } from "../types/analytics";
import { type Scope, scopeQuery } from "./usage";

/** The analytics of the last `days` local days of the server, of the cards in `scope`. */
export function analytics(days: number, scope: Scope = {}): Promise<Analytics> {
  const query = scopeQuery(scope);
  query.set("days", String(days));
  return get<Analytics>(`/v1/analytics?${query}`);
}
