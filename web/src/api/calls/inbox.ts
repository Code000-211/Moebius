/**
 * The decisions of the inbox that are not about a card or its change (those are in `board.ts`): merging or dismissing a
 * proposal of the Retro or a takeover of the decision model, and confirming or dismissing a grilling.
 */
import { del, post, put } from "../http";
import type { InboxItem } from "../types/board";
import type { GrillingIssue } from "../types/inbox";

export async function mergeProposal(proposal: number): Promise<void> {
  await post<unknown>(`/v1/proposals/${proposal}/merge`, {});
}

export async function dismissProposal(proposal: number): Promise<void> {
  await post<unknown>(`/v1/proposals/${proposal}/dismiss`, {});
}

export async function mergeTakeover(takeover: number): Promise<void> {
  await post<unknown>(`/v1/takeovers/${takeover}/merge`, {});
}

export async function dismissTakeover(takeover: number): Promise<void> {
  await post<unknown>(`/v1/takeovers/${takeover}/dismiss`, {});
}

/** Confirms a grilling with the issues as the person left them; a card is made of each. */
export async function confirmGrilling(grilling: number, issues: GrillingIssue[]): Promise<void> {
  await post<unknown>(`/v1/grillings/${grilling}/confirm`, { issues });
}

/** Deletes a grilling with its chat; the issues it filed stay as cards. */
export async function deleteGrilling(grilling: number): Promise<void> {
  await del<unknown>(`/v1/grillings/${grilling}`);
}

export async function dismissGrilling(grilling: number): Promise<void> {
  await post<unknown>(`/v1/grillings/${grilling}/dismiss`, {});
}

/** Changes the people an inbox item is given to from `before` to `after`, one person at a time. */
export async function changeInboxAssignees(item: number, before: string[], after: string[]) {
  const had = new Set(before);
  const wants = new Set(after);
  const path = (handle: string) => `/v1/inbox/${item}/assignees/${encodeURIComponent(handle)}`;
  for (const handle of before) if (!wants.has(handle)) await del<InboxItem>(path(handle));
  for (const handle of after) if (!had.has(handle)) await put<InboxItem>(path(handle), undefined);
}
