/** The call the Fleet reads for each tile: the run itself, for when it started. */
import { get } from "../http";
import type { Run } from "../types/cards";

export function run(id: number): Promise<Run> {
  return get<Run>(`/v1/runs/${id}`);
}
