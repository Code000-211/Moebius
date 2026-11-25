/** The calls of the software factory (ADR 0032): a repository's stale settings, a project's backlog and its sync, and the digest. */
import { del, get, post, put } from "../http";
import type {
  Backlog,
  BacklogEdit,
  Flow,
  SyncReport,
  TemplateSync,
  Templates,
  Upkeep,
} from "../types/factory";

export function readUpkeep(repository: number): Promise<Upkeep> {
  return get<Upkeep>(`/v1/repositories/${repository}/upkeep`);
}

/** Sets the days of a repository's stale handling (an owner); 0 turns one off. */
export function editUpkeep(repository: number, upkeep: Upkeep): Promise<Upkeep> {
  return put<Upkeep>(`/v1/repositories/${repository}/upkeep`, upkeep);
}

export function readFlow(repository: number): Promise<Flow> {
  return get<Flow>(`/v1/repositories/${repository}/flow`);
}

/** Sets how a repository's reviews and unbound pull requests flow (an owner). */
export function editFlow(repository: number, flow: Flow): Promise<Flow> {
  return put<Flow>(`/v1/repositories/${repository}/flow`, flow);
}

/** The pull request and comment templates in effect for a repository (ADR 0034). */
export function readTemplates(repository: number): Promise<Templates> {
  return get<Templates>(`/v1/repositories/${repository}/templates`);
}

/** Opens the pull request that brings the repository's copies of the templates up to date; it waits for the owner. */
export function syncTemplates(repository: number): Promise<TemplateSync> {
  return post<TemplateSync>(`/v1/repositories/${repository}/templates/sync`, {});
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
