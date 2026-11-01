/**
 * The decisions of the inbox that are not about a card or its change (those are in `board.ts`): merging or dismissing a
 * proposal of the Retro or a takeover of the decision model, and confirming or dismissing a grilling.
 */
import { post } from "../http";
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

export async function dismissGrilling(grilling: number): Promise<void> {
  await post<unknown>(`/v1/grillings/${grilling}/dismiss`, {});
}
