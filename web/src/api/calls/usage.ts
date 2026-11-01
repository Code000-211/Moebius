/** The call about what the model calls used. */
import { get } from "../http";
import type { UsageReport } from "../types/usage";

/** The usage of the last `days` local days of the server, today included. */
export function usage(days: number): Promise<UsageReport> {
  return get<UsageReport>(`/v1/usage?days=${days}`);
}
