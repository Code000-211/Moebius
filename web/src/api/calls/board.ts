/**
 * The calls about the boards: the snapshot of everything the shell shows, a card's history and runs, what a person
 * decides about a card or its change, a new issue, and the script behind a column (read, saved and given up).
 */
import { del, get, post, put } from "../http";
import type { Card, Snapshot } from "../types/board";
import type { HistoryEntry, Run, ScriptDetail } from "../types/cards";

export function snapshot(): Promise<Snapshot> {
  return get<Snapshot>("/v1/snapshot");
}

/** The card's timeline, oldest first. */
export function history(card: number): Promise<HistoryEntry[]> {
  return get<HistoryEntry[]>(`/v1/cards/${card}/history`);
}

/** The card's runs, oldest first. */
export function cardRuns(card: number): Promise<Run[]> {
  return get<Run[]>(`/v1/cards/${card}/runs`);
}

/** Creates an issue in a repository; on a forge the server opens it there. */
export function createIssue(repository: number, title: string, body: string): Promise<Card> {
  return post<Card>(`/v1/repositories/${repository}/issues`, { title, body });
}

/** Approves an issue that waits in the backlog: the card moves to Ready. */
export function approve(card: number): Promise<Card> {
  return post<Card>(`/v1/cards/${card}/approve`, {});
}

/** Merges an open change; the answer is the merge commit. */
export function merge(change: number): Promise<{ commit: string }> {
  return post<{ commit: string }>(`/v1/changes/${change}/merge`, {});
}

/** Sends a change back to coding, with the reason as the card's note. */
export function reject(change: number, reason: string): Promise<Card> {
  return post<Card>(`/v1/changes/${change}/reject`, { reason });
}

/** Gives a stuck card, or one whose pull request was closed, a fresh start. */
export function retry(card: number): Promise<Card> {
  return post<Card>(`/v1/cards/${card}/retry`, {});
}

/** The script a column runs for a repository. The column's name may have spaces. */
export function script(repository: number, column: string): Promise<ScriptDetail> {
  return get<ScriptDetail>(`/v1/repositories/${repository}/scripts/${encodeURIComponent(column)}`);
}

/**
 * Replaces the column's script for the repository. The server checks it as `moebius check-script` does and answers
 * 422 with the check's message when it fails.
 */
export function saveScript(
  repository: number,
  column: string,
  text: string,
  reason = "",
): Promise<ScriptDetail> {
  return put<ScriptDetail>(`/v1/repositories/${repository}/scripts/${encodeURIComponent(column)}`, {
    text,
    reason,
  });
}

/** Removes the repository's own script for the column, so the column runs the built-in one again. */
export function deleteScript(repository: number, column: string): Promise<ScriptDetail> {
  return del<ScriptDetail>(`/v1/repositories/${repository}/scripts/${encodeURIComponent(column)}`);
}
