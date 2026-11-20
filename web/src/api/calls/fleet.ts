/** The calls the Fleet reads: a tile's run, for when it started, and the runs that have ended. */
import { get } from "../http";
import type { Run } from "../types/cards";
import type { FinishedRun } from "../types/runs";

export function run(id: number): Promise<Run> {
  return get<Run>(`/v1/runs/${id}`);
}

/** The runs of cards that have ended, the one that ended last first. */
export function finishedRuns(limit = 500): Promise<FinishedRun[]> {
  return get<FinishedRun[]>(`/v1/runs/finished?limit=${limit}`);
}
