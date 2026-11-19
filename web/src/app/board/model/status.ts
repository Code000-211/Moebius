/**
 * What state a card is in: its changes, whether a run is going or a person holds it, and the one status the board shows.
 */
import type { Board, Card, CardChange, Held, InboxItem } from "../../../api/types/board";
import { STUCK } from "./inbox";

/** Every open change of a card, one per repository in the order they are merged; a server that sends one has its own. */
export function changesOfCard(card: Card): CardChange[] {
  return card.changes ?? (card.change ? [card.change] : []);
}

/** A change's repository, said when a card has changes in several, so that they can be told apart. */
export function inRepository(card: Card, change: CardChange): string {
  const several = changesOfCard(card).length > 1;
  return several && change.repository ? ` in ${change.repository}` : "";
}

/** Whether a card is being worked on: its latest run starts or runs. */
export function isLive(card: Card): boolean {
  return card.run?.status === "starting" || card.run?.status === "running";
}

/**
 * `held` is a card a person or the Moebius agent blocked: the board works on nothing for it. `blocked` is a card that
 * waits for the issues it depends on. They are two states and have two words, "Blocked" and "Waiting on issues".
 */
export type CardStatus = "held" | "live" | "needs-you" | "stuck" | "blocked" | "waiting" | "idle";

export const STATUS_LABELS: Record<CardStatus, string> = {
  held: "Blocked",
  live: "Live",
  "needs-you": "Your turn",
  stuck: "Stuck",
  blocked: "Waiting on issues",
  waiting: "Waiting",
  idle: "Idle",
};

/** Whether a person or the Moebius agent holds the card, so that the board leaves it alone. */
export function isHeld(card: Card): boolean {
  return card.held !== undefined && card.held !== null;
}

/**
 * Who held a card, in words: "you" cannot be said, because the server does not know which person it was, so a card
 * a person held was held "by a person".
 */
export function heldBy(held: Held): string {
  return held.by.startsWith("agent:") ? `by ${held.by.slice("agent:".length)}` : "by a person";
}

/** The line a held card is told by: why it was blocked and by whom, or only by whom when no reason was given. */
export function heldLine(held: Held): string {
  const reason = held.reason?.trim() ?? "";
  return reason === "" ? `Blocked ${heldBy(held)}` : `${reason} (blocked ${heldBy(held)})`;
}

/** What a card is doing, the first that applies: held, a run going, a person asked, stuck, waiting on issues, or nothing. */
export function cardStatus(card: Card, item: InboxItem | undefined, board: Board): CardStatus {
  if (isHeld(card)) return "held";
  if (isLive(card)) return "live";
  if (item?.kind === STUCK) return "stuck";
  if (item) return "needs-you";
  if (card.blocked_by.length > 0) return "blocked";
  if (board.waiting[String(card.id)] !== undefined) return "waiting";
  return "idle";
}
