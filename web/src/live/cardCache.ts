/**
 * Writes what the server answered to a person's own change into the cache, so the change shows at once instead of
 * after the next read of the snapshot. The answer of a card's change is the card; it replaces the card in the
 * snapshot (and a new card is added to its board). A read of the snapshot that is already on its way began before
 * the answer and would put the old value back, so it is cancelled first; the event the change makes marks the
 * snapshot stale and `coalesce.ts` reads it once more.
 */
import type { QueryClient } from "@tanstack/react-query";
import { keys } from "../api/keys";
import type { Board, Card, Snapshot } from "../api/types/board";

/** Whether an answer of the API is a card, which is how the calls that change a card answer. */
export function isCard(answer: unknown): answer is Card {
  if (typeof answer !== "object" || answer === null) return false;
  const card = answer as Partial<Card>;
  return (
    typeof card.id === "number" && typeof card.column === "string" && Array.isArray(card.labels)
  );
}

function changeBoards(queryClient: QueryClient, change: (boards: Board[]) => Board[]): void {
  void queryClient.cancelQueries({ queryKey: keys.snapshot });
  queryClient.setQueryData<Snapshot>(keys.snapshot, (snapshot) =>
    snapshot ? { ...snapshot, boards: change(snapshot.boards) } : snapshot,
  );
}

/**
 * What a board shows of a card beyond the card itself (its runs, blockers, change) is not in the answer of a change,
 * which is the plain card: a card the board lacks gets the values of a card with nothing going on, until the board
 * is read again. A sub-issue counts, as the server says of one a person made.
 */
function withView(card: Card): Card {
  const nothing: Partial<Card> = {
    blocked_by: [],
    counts_as_sub_issue: true,
    rounds: 0,
    tokens: 0,
    refusals: 0,
    run: null,
    change: null,
  };
  return { ...nothing, ...card };
}

/** Puts `card`, the answer of a change, in place of the card it replaces on its board; a card the board lacks is added. */
export function putCard(queryClient: QueryClient, card: Card): void {
  changeBoards(queryClient, (boards) =>
    boards.map((board) => {
      if (board.repository.id !== card.repository_id) return board;
      const at = board.cards.findIndex((c) => c.id === card.id);
      const cards =
        at < 0
          ? [...board.cards, withView(card)]
          : board.cards.map((c) => (c.id === card.id ? { ...c, ...card } : c));
      return { ...board, cards };
    }),
  );
}

/** Puts what an answer says about a card (the blockers a call answers with) into the card the cache has. */
export function patchCard(queryClient: QueryClient, id: number, patch: Partial<Card>): void {
  changeBoards(queryClient, (boards) =>
    boards.map((board) =>
      board.cards.some((c) => c.id === id)
        ? { ...board, cards: board.cards.map((c) => (c.id === id ? { ...c, ...patch } : c)) }
        : board,
    ),
  );
}

/** Takes a deleted card off its board. */
export function removeCard(queryClient: QueryClient, id: number): void {
  changeBoards(queryClient, (boards) =>
    boards.map((board) =>
      board.cards.some((c) => c.id === id)
        ? { ...board, cards: board.cards.filter((c) => c.id !== id) }
        : board,
    ),
  );
}

/** Puts every card among `answers` (the settled results of several calls, or an answer of any kind) into the snapshot. */
export function putCards(queryClient: QueryClient, answers: readonly unknown[]): void {
  for (const answer of answers) if (isCard(answer)) putCard(queryClient, answer);
}
