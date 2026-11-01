/** The calls about one run: the run, steering it, what it and a change changed, and the ports of the server. */
import { get, post } from "../http";
import type { Run } from "../types/cards";
import type { Diff, Port } from "../types/runs";

export function run(id: number): Promise<Run> {
  return get<Run>(`/v1/runs/${id}`);
}

/** A person's message to a running run; the agent answers it before the run ends. */
export function steer(id: number, message: string): Promise<Run> {
  return post<Run>(`/v1/runs/${id}/steer`, { message });
}

export function runDiff(id: number): Promise<Diff> {
  return get<Diff>(`/v1/runs/${id}/diff`);
}

export function changeDiff(id: number): Promise<Diff> {
  return get<Diff>(`/v1/changes/${id}/diff`);
}

/** The ports that runs, chats and terminals listen on, by port. */
export function ports(): Promise<Port[]> {
  return get<Port[]>("/v1/ports");
}
