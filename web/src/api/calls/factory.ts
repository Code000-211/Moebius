/** The calls of the software factory (ADR 0031): a repository's stale settings, a project's backlog and its sync, and the digest. */
import { del, get, post, put } from "../http";
import type { Backlog, BacklogEdit, SyncReport, Upkeep } from "../types/factory";

export function readUpkeep(repository: number): Promise<Upkeep> {
  return get<Upkeep>(`/v1/repositories/${repository}/upkeep`);
}

/** Sets the days of a repository's stale handling (an owner); 0 turns one off. */
export function editUpkeep(repository: number, upkeep: Upkeep): Promise<Upkeep> {
  return put<Upkeep>(`/v1/repositories/${repository}/upkeep`, upkeep);
}

/** The project's backlog, or null when it has none. */
export function readBacklog(project: number): Promise<Backlog | null> {
  return get<Backlog | null>(`/v1/projects/${project}/backlog`);
}

export function editBacklog(project: number, edit: BacklogEdit): Promise<Backlog> {
  return put<Backlog>(`/v1/projects/${project}/backlog`, edit);
}

export async function removeBacklog(project: number): Promise<void> {
  await del<unknown>(`/v1/projects/${project}/backlog`);
}

/** Makes the issues of the backlog's items; with `dryRun` nothing is written and the report says what would be. */
export function syncBacklog(project: number, dryRun: boolean): Promise<SyncReport> {
  return post<SyncReport>(`/v1/projects/${project}/backlog/sync`, { dry_run: dryRun });
}

/** Resolves a digest's inbox item once it is read. */
export async function dismissDigest(item: number): Promise<void> {
  await post<unknown>(`/v1/digest/${item}/dismiss`, {});
}
