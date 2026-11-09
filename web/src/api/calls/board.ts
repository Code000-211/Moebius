/**
 * The calls about the boards: the snapshot of everything the shell shows, a card's history and runs, what a person
 * decides about a card or its change, a new issue, and the script behind a column (read, saved and given up).
 */
import { del, get, post, put } from "../http";
import type { Card, Comment, Snapshot } from "../types/board";
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

/** A new issue's card, and what the forge refused after the issue was made (its parent, labels or blockers), when something. */
export type CreatedIssue = Card & { warnings?: string[] };

/** Creates an issue in a repository, a sub-issue of `parent` (an issue number) when one is given; on a forge the server opens it there. */
export function createIssue(
  repository: number,
  title: string,
  body: string,
  parent?: number | null,
): Promise<CreatedIssue> {
  return post<CreatedIssue>(`/v1/repositories/${repository}/issues`, {
    title,
    body,
    ...(parent != null ? { parent } : {}),
  });
}

/** Changes an issue's title and body, on its forge too. */
export function editCard(card: number, title: string, body: string): Promise<Card> {
  return put<Card>(`/v1/cards/${card}`, { title, body });
}

/** Makes the issue a sub-issue of the issue with number `parent`, or gives it none. */
export function setParent(card: number, parent: number | null): Promise<Card> {
  return put<Card>(`/v1/cards/${card}/parent`, { parent });
}

/** Replaces the issue's labels. */
export function setLabels(card: number, labels: string[]): Promise<Card> {
  return put<Card>(`/v1/cards/${card}/labels`, { labels });
}

/** Makes the issue with number `blocker` block the card's issue; refused when that makes a cycle. */
export function addBlocker(card: number, blocker: number): Promise<number[]> {
  return post<number[]>(`/v1/cards/${card}/blockers`, { blocker });
}

/** Stops the issue with number `blocker` from blocking the card's issue. */
export function removeBlocker(card: number, blocker: number): Promise<number[]> {
  return del<number[]>(`/v1/cards/${card}/blockers/${blocker}`);
}

/** Claims the issue for the person; refused when somebody else did. */
export function claim(card: number): Promise<Card> {
  return post<Card>(`/v1/cards/${card}/claim`, {});
}

/** Releases the claim on the issue. */
export function release(card: number): Promise<Card> {
  return del<Card>(`/v1/cards/${card}/claim`);
}

/** Closes the issue: its card goes to Done. */
export function closeIssue(card: number): Promise<Card> {
  return post<Card>(`/v1/cards/${card}/close`, {});
}

/** Reopens a closed issue: its card goes back to the backlog. */
export function reopenIssue(card: number): Promise<Card> {
  return post<Card>(`/v1/cards/${card}/reopen`, {});
}

/** The comments of the issue, oldest first. */
export function comments(card: number): Promise<Comment[]> {
  return get<Comment[]>(`/v1/cards/${card}/comments`);
}

/** Writes a comment on the issue. */
export function comment(card: number, body: string): Promise<Comment> {
  return post<Comment>(`/v1/cards/${card}/comments`, { body });
}

/** Approves an issue that waits in the backlog: the card moves to Not started. */
export function approve(card: number): Promise<Card> {
  return post<Card>(`/v1/cards/${card}/approve`, {});
}

/** Merges an open change; the answer is the merge commit. */
export function merge(change: number): Promise<{ commit: string }> {
  return post<{ commit: string }>(`/v1/changes/${change}/merge`, {});
}

/** Sends a change back to coding, with the reason as the card's note; a run's change has no card. */
export function reject(change: number, reason: string): Promise<Card | null> {
  return post<Card | null>(`/v1/changes/${change}/reject`, { reason });
}

/** Gives a stuck card, or one whose pull request was closed, a fresh start. */
export function retry(card: number): Promise<Card> {
  return post<Card>(`/v1/cards/${card}/retry`, {});
}

/** Holds a card: its runs are stopped and the board works on nothing for it until it is released. */
export function block(card: number, reason: string): Promise<Card> {
  return post<Card>(`/v1/cards/${card}/block`, { reason });
}

/** Releases a held card. */
export function unblock(card: number): Promise<Card> {
  return post<Card>(`/v1/cards/${card}/unblock`, {});
}

/** Deletes a card with everything stored for it; its issue on a forge stays. */
export function deleteCard(card: number): Promise<Card> {
  return del<Card>(`/v1/cards/${card}`);
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
