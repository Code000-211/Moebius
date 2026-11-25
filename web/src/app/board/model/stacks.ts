/**
 * Cards that have sub-issues: who is the parent of whom, the stack under a card, whether an issue is closed and which
 * card the board starts next.
 */
import { isDone } from "../../../api/columns";
import type { Board, Card } from "../../../api/types/board";
import { plural } from "../../../ui/lib/format";
import { isHeld } from "./status";

/**
 * The number of the issue `card` is a sub-issue of, as the server counts it: none for a card that names a parent but
 * does not count as its sub-issue, which is an ordinary card of its own here.
 */
export function parentNumber(card: Card): number | null {
  return card.counts_as_sub_issue ? card.parent : null;
}

/** The card the issue number `parent` names on the board, if it has one. */
function cardNumbered(board: Board, parent: number | null | undefined): Card | undefined {
  return parent == null ? undefined : board.cards.find((card) => card.number === parent);
}

/** The card that is the parent of `card`, if it is on the board. */
export function parentOf(board: Board, card: Card): Card | undefined {
  return cardNumbered(board, parentNumber(card));
}

/** The card at the top of the stack `card` is in: itself when it has no parent on the board. */
export function rootOf(board: Board, card: Card): Card {
  let top = card;
  // A cycle cannot be made through the server, but a board that has one must not hang the page.
  for (let depth = 0; depth < 32; depth++) {
    const up = parentOf(board, top);
    if (!up || up.id === card.id) return top;
    top = up;
  }
  return top;
}

/** The issues below `card`, at any depth, in the order of their numbers. */
export function descendants(board: Board, card: Card): Card[] {
  const found: Card[] = [];
  const seen = new Set<number>([card.id]);
  const walk = (of: Card) => {
    for (const child of board.cards
      .filter((c) => parentNumber(c) === of.number && c.id !== of.id)
      .sort((a, b) => a.number - b.number)) {
      if (seen.has(child.id)) continue;
      seen.add(child.id);
      found.push(child);
      walk(child);
    }
  };
  walk(card);
  return found;
}

/** Whether the issue is closed: it was closed, or its card is in Done. */
export function isClosed(card: Card): boolean {
  return card.closed || isDone(card);
}

/** A card with sub-issues, and how its stack stands. */
export interface Stack {
  /** The sub-issues the filters keep, in order. */
  shown: Card[];
  /** How many sub-issues it has in all. */
  total: number;
  /** How many of them are done. */
  done: number;
}

/** The words of a stack's count on its card: "2 of 5 sub-issues done". */
export function stackWords(stack: Stack): string {
  return `${stack.done} of ${plural(stack.total, "sub-issue", "sub-issues")} done`;
}

/**
 * The card the board starts next: the first in Not started that nothing blocks and nobody holds, and that is not a
 * parent with sub-issues still open, which waits for them instead.
 */
export function nextCard(cards: readonly Card[], all: readonly Card[] = cards): number | undefined {
  return cards.find(
    (card) =>
      card.blocked_by.length === 0 &&
      !isHeld(card) &&
      !all.some((other) => parentNumber(other) === card.number && !isClosed(other)),
  )?.id;
}
