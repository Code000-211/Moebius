import { QueryClient } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";
import { keys } from "../api/keys";
import type { Board, Card, Snapshot } from "../api/types/board";
import { isCard, patchCard, putCard, putCards, removeCard } from "./cardCache";

const card = (id: number, extra: Partial<Card> = {}) =>
  ({
    id,
    repository_id: 1,
    number: id,
    title: `Issue ${id}`,
    column: "Not started",
    labels: [],
    blocked_by: [],
    priority: "none",
    ...extra,
  }) as unknown as Card;
const board = (id: number, cards: Card[]) =>
  ({ repository: { id }, cards, waiting: {}, changes: [] }) as unknown as Board;

function held(...boards: Board[]) {
  const client = new QueryClient();
  client.setQueryData<Snapshot>(keys.snapshot, { boards, harness_changes: [], inbox: [] });
  return client;
}
const cards = (client: QueryClient, at = 0) =>
  client.getQueryData<Snapshot>(keys.snapshot)?.boards[at]?.cards ?? [];

describe("the answer of a change in the snapshot", () => {
  it("replaces the card and leaves the others", () => {
    const client = held(board(1, [card(1), card(2)]));
    putCard(client, card(2, { priority: "high" }));
    expect(cards(client).map((c) => [c.id, c.priority])).toEqual([
      [1, "none"],
      [2, "high"],
    ]);
  });

  it("adds a new card to the board of its repository", () => {
    const client = held(board(1, [card(1)]), board(2, []));
    putCard(client, card(9, { repository_id: 2 }));
    expect(cards(client, 0).map((c) => c.id)).toEqual([1]);
    expect(cards(client, 1).map((c) => c.id)).toEqual([9]);
  });

  it("patches a card, takes one away, and does nothing without a snapshot", () => {
    const client = held(board(1, [card(1), card(2)]));
    patchCard(client, 1, { blocked_by: [2] });
    expect(cards(client)[0]?.blocked_by).toEqual([2]);
    removeCard(client, 2);
    expect(cards(client).map((c) => c.id)).toEqual([1]);
    const empty = new QueryClient();
    putCard(empty, card(1));
    expect(empty.getQueryData(keys.snapshot)).toBeUndefined();
  });

  it("tells a card from other answers", () => {
    expect(isCard(card(1))).toBe(true);
    expect(isCard([3, 7])).toBe(false);
    expect(isCard({ commit: "abc" })).toBe(false);
    expect(isCard(null)).toBe(false);
    const client = held(board(1, [card(1)]));
    putCards(client, [{ commit: "abc" }, card(1, { priority: "low" })]);
    expect(cards(client)[0]?.priority).toBe("low");
  });

  it("is not undone by a read that began before the answer", async () => {
    const client = held(board(1, [card(1)]));
    let finish: (snapshot: Snapshot) => void = () => {};
    const read = client.fetchQuery({
      queryKey: keys.snapshot,
      queryFn: () => new Promise<Snapshot>((resolve) => (finish = resolve)),
      staleTime: 0,
    });
    read.catch(() => {});
    putCard(client, card(1, { priority: "high" }));
    finish({ boards: [board(1, [card(1)])], harness_changes: [], inbox: [] });
    await read.catch(() => {});
    expect(cards(client)[0]?.priority).toBe("high");
  });
});
