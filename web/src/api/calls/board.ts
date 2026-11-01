/** The calls about the boards: the snapshot of everything the shell shows. */
import { get } from "../http";
import type { Snapshot } from "../types/board";

export function snapshot(): Promise<Snapshot> {
  return get<Snapshot>("/v1/snapshot");
}
