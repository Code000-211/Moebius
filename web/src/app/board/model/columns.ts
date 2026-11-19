/**
 * The columns of a board and the cards each shows: their names, the order of the cards, the stacks under them after the
 * filters, and the card the dock shows.
 */
import type { Board, Card, InboxItem } from "../../../api/types/board";
import { type Filters, matchesCard, priorityRank } from "./filters";
import { inboxOf } from "./inbox";
import { descendants, isClosed, rootOf, type Stack } from "./stacks";
import { isLive } from "./status";

/** The columns of the default workflow, left to right. */
export const COLUMNS = [
  "Backlog",
  "Not started",
  "In progress",
  "In review",
  "Ready to merge",
  "Done",
] as const;

/** How many cards Done lists, the highest numbers first, because it keeps growing. */
export const DONE_SHOWN = 10;

/** The names of the columns a board shows: the default workflow's, Backlog always so that issues have a place to wait, and any other column a card is in, last. */
export function boardColumns(board: Board): string[] {
  const columns: string[] = [...COLUMNS];
  for (const card of board.cards) {
    if (!columns.includes(card.column)) columns.push(card.column);
  }
  return columns;
}

/**
 * Cards in the order Display's sorting asks for: by number (the default; Done newest first), newest first, or by
 * title. The cards given are not changed.
 */
export function orderCards(cards: readonly Card[], sort: string, column?: string): Card[] {
  const ordered = [...cards];
  if (sort === "newest") {
    return ordered.sort((a, b) => b.created_at - a.created_at || b.number - a.number);
  }
  if (sort === "title") {
    return ordered.sort((a, b) => a.title.localeCompare(b.title) || a.number - b.number);
  }
  if (sort === "priority") {
    return ordered.sort(
      (a, b) => priorityRank(a.priority) - priorityRank(b.priority) || a.number - b.number,
    );
  }
  if (sort === "due") {
    // Dated cards first, the earliest day first; the cards without a date keep their order by number.
    return ordered.sort(
      (a, b) =>
        (a.due === null ? 1 : 0) - (b.due === null ? 1 : 0) ||
        (a.due ?? "").localeCompare(b.due ?? "") ||
        a.number - b.number,
    );
  }
  ordered.sort((a, b) => a.number - b.number);
  return column === "Done" ? ordered.reverse() : ordered;
}

export interface ColumnCards {
  /** The name of the column. */
  column: string;
  /** The cards shown, in order: the tops of the stacks, and the cards that have no parent on the board. */
  cards: Card[];
  /** How many cards the column has after the filters (Done shows only some of them). */
  total: number;
  /** The stack of each card shown that has sub-issues, by card id. */
  stacks: Map<number, Stack>;
}

/**
 * The columns of a board with the cards each shows: filtered, ordered by number, Done newest first and cut. A card with
 * sub-issues lies in its own column with its sub-issues in a stack under it, wherever they are; a sub-issue is not a card
 * of its column. The filters apply to the sub-issues: the agent, the status and the label to each of them, and the column
 * to the column it is in. A card whose own column is filtered out is not shown, and a card that does not pass the other
 * filters is shown only for the sub-issues that do.
 */
export function groupByColumn(
  board: Board,
  inbox: readonly InboxItem[],
  filters: Filters,
  sort = "",
): ColumnCards[] {
  const inColumn = (card: Card) => filters.column === "" || card.column === filters.column;
  const tops = board.cards.filter((card) => rootOf(board, card).id === card.id);
  const stacks = new Map<number, Stack>();
  const kept = tops.filter((card) => {
    const below = descendants(board, card);
    const shown = below.filter(
      (child) => inColumn(child) && matchesCard(child, filters, inbox, board),
    );
    if (below.length > 0) {
      stacks.set(card.id, {
        shown,
        total: below.length,
        done: below.filter(isClosed).length,
      });
    }
    return inColumn(card) && (matchesCard(card, filters, inbox, board) || shown.length > 0);
  });
  return boardColumns(board)
    .filter((column) => filters.column === "" || column === filters.column)
    .map((column) => {
      const cards = orderCards(
        kept.filter((card) => card.column === column),
        sort,
        column,
      );
      return {
        column,
        cards: column === "Done" ? cards.slice(0, DONE_SHOWN) : cards,
        total: cards.length,
        stacks,
      };
    });
}

/**
 * The card the dock shows. A card chosen with `selected` (a card id, or "none" for a dock closed on purpose) wins;
 * a chosen card that is not on the board — it was deleted — leaves the dock closed; without a choice it is the first
 * live card, else the first that waits for a person, else the first not done.
 */
export function inspectedCard(
  board: Board,
  inbox: readonly InboxItem[],
  selected: string | null,
): Card | undefined {
  if (selected === "none") return undefined;
  if (selected !== null) {
    return board.cards.find((card) => String(card.id) === selected);
  }
  return (
    board.cards.find(isLive) ??
    board.cards.find((card) => inboxOf(inbox, card) !== undefined) ??
    board.cards.find((card) => card.column !== "Done")
  );
}
