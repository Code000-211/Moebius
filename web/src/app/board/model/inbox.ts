/**
 * The inbox items that stop a card, as the board reads them: which kinds there are, the open item of a card (found
 * once per read of the inbox) and what a stopped merge says.
 */
import type { Card, InboxItem } from "../../../api/types/board";

/** The kinds of inbox item that stop a card for a person. */
export const STUCK = "stuck";
export const CHANGE_CLOSED = "change_closed";
export const READY_TO_MERGE = "ready_to_merge";
export const APPROVE_ISSUE = "approve_issue";
/** A pull request someone on another server opened for an issue they took: the card follows it and the owner may review and merge it. */
export const OUTSIDE_CHANGE = "outside_change";
/** The assignee of an issue that was taken with `/take` wrote `/done`: the owner closes the issue, gives it back or releases it. */
export const DONE_ASKED = "done_asked";
/** The Reviewer read a followed pull request three times and its author asked again: the owner looks, with the same choices as for the request itself. */
export const REVIEW_ROUNDS_SPENT = "review_rounds_spent";
/** A followed pull request could not be given to the Reviewer (too large, a binary file, or git failed): the owner reads it, with the same choices as for the request itself. */
export const REVIEW_NOT_POSSIBLE = "review_not_possible";
/** What an outside issue that waits for a person's approval is called, on every row, card and button of the app. */
export const WAITING_FOR_APPROVAL = "Waiting for approval";

/** The pale words of a card that waits for a person, by the kind of its open item; "Your turn" for a kind with no word of its own. */
export function waitingWords(kind: string | undefined): string {
  switch (kind) {
    case APPROVE_ISSUE:
      return WAITING_FOR_APPROVAL;
    case READY_TO_MERGE:
      return "Ready to merge";
    case OUTSIDE_CHANGE:
      return "Pull request to review";
    case CHANGE_CLOSED:
      return "Closed without a merge";
    case DONE_ASKED:
      return "Says it is done";
    case REVIEW_ROUNDS_SPENT:
      return "Reviewed three times";
    case REVIEW_NOT_POSSIBLE:
      return "Could not be reviewed";
    default:
      return "Your turn";
  }
}

/** A merge of a card's changes stopped after another change of the card was merged; it informs, and is no turn. */
export const MERGE_STOPPED = "merge_stopped";

/**
 * The open item of every card of one read of the inbox, kept so that a board asks for a hundred cards without walking
 * the inbox a hundred times. It is held against the array itself, which the query hands out unchanged until the inbox
 * is read again.
 */
const openItems = new WeakMap<readonly InboxItem[], Map<number, InboxItem>>();

/** The open inbox item of a card, if it has one. */
export function inboxOf(inbox: readonly InboxItem[], card: Card): InboxItem | undefined {
  let byCard = openItems.get(inbox);
  if (byCard === undefined) {
    byCard = new Map();
    for (const item of inbox) {
      if (item.resolved_at !== null || item.card_id === null) continue;
      const held = byCard.get(item.card_id);
      // A stopped merge only says what is merged; the item that asks for a person's turn stays the card's item. Of
      // several changes to merge, the first in the order they are merged comes first, since a later one is refused.
      if (item.kind === MERGE_STOPPED && held !== undefined) continue;
      // The request's own item comes first; this one is about it, and neither hides the other's choices.
      if (
        item.kind === OUTSIDE_CHANGE &&
        (held?.kind === REVIEW_ROUNDS_SPENT || held?.kind === REVIEW_NOT_POSSIBLE)
      )
        continue;
      if (item.kind === READY_TO_MERGE && held?.kind === READY_TO_MERGE && held.id < item.id)
        continue;
      byCard.set(item.card_id, item);
    }
    openItems.set(inbox, byCard);
  }
  return byCard.get(card.id);
}

/** The open `merge_stopped` item of a card, if a merge of its changes stopped after some were merged. */
export function mergeStoppedOf(inbox: readonly InboxItem[], card: Card): InboxItem | undefined {
  return inbox.find(
    (item) => item.kind === MERGE_STOPPED && item.card_id === card.id && item.resolved_at === null,
  );
}

/** What a `merge_stopped` item says: the repositories whose changes are merged and those still open, and why it stopped. */
export function mergeStopped(item: InboxItem): {
  repository: string;
  reason: string;
  merged: string[];
  open: string[];
} {
  const names = (value: unknown): string[] =>
    Array.isArray(value) ? value.filter((v): v is string => typeof v === "string") : [];
  const word = (value: unknown): string => (typeof value === "string" ? value : "");
  return {
    repository: word(item.data.repository),
    reason: word(item.data.reason),
    merged: names(item.data.merged),
    open: names(item.data.open),
  };
}
