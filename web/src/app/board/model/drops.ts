/**
 * What a drop of a card on a column means, and why it may be refused.
 */
import { classOf, type HasColumns } from "../../../api/columns";
import type { Card, InboxItem } from "../../../api/types/board";
import { APPROVE_ISSUE, CHANGE_CLOSED, READY_TO_MERGE, STUCK } from "./inbox";
import { isHeld, isLive } from "./status";

/** What a drop of a card on a column means. */
export type Drop =
  | { kind: "none" }
  | { kind: "approve" }
  | { kind: "retry" }
  | { kind: "merge"; change: number }
  /** Sending a change back needs a reason, which the dock asks for. */
  | { kind: "reject"; change: number }
  | { kind: "refused"; reason: string };

function changeOfItem(item: InboxItem): number | null {
  const change = item.data.change;
  return typeof change === "number" ? change : null;
}

/**
 * What dropping `card` on the column `to` does; a custom column counts as the built-in one it counts as. A person decides only what waits in the inbox: an issue to approve
 * goes to Not started, a change to merge goes to Done or back to In progress (rejected), and a stuck card, or one whose pull
 * request was closed, goes to In progress (retried). Any other drop is refused with the reason, since the board moves
 * every other card itself when its column's script says so.
 */
export function dropRule(
  card: Card,
  to: string,
  item: InboxItem | undefined,
  /** The columns of the board, to know the class of a custom column: a drop on it means what a drop on its class means. */
  columns?: HasColumns,
): Drop {
  const kind = item?.kind;
  const target = classOf(columns, to);
  if (kind === READY_TO_MERGE && item) {
    const change = changeOfItem(item);
    if (target === "Done" || target === "In progress") {
      if (change === null) {
        return { kind: "refused", reason: "This card has no change to decide on yet." };
      }
      return target === "Done" ? { kind: "merge", change } : { kind: "reject", change };
    }
  }
  if (kind === APPROVE_ISSUE && target === "Not started") return { kind: "approve" };
  if ((kind === STUCK || kind === CHANGE_CLOSED) && target === "In progress")
    return { kind: "retry" };
  if (card.column === to) return { kind: "none" };
  if (isLive(card)) {
    return {
      kind: "refused",
      reason: "Moebius is working on this card. It moves when its run ends.",
    };
  }
  switch (kind) {
    case READY_TO_MERGE:
      return {
        kind: "refused",
        reason:
          "This change waits for you: drop it on Done to merge it or on In progress to send it back.",
      };
    case APPROVE_ISSUE:
      return {
        kind: "refused",
        reason: "This issue waits for your approval: drop it on Not started to approve it.",
      };
    case STUCK:
    case CHANGE_CLOSED:
      return { kind: "refused", reason: "This card is stuck: drop it on In progress to retry it." };
    default:
      if (isHeld(card)) {
        return {
          kind: "refused",
          reason: "This card is blocked. Unblock it from its menu and the board moves it again.",
        };
      }
      return {
        kind: "refused",
        reason: `Nobody decides this card now. The board moves it from ${card.column} when the column's script says so.`,
      };
  }
}
