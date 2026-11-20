import { describe, expect, it } from "vitest";
import type { Board, Snapshot } from "../../api/types/board";
import type { Notice } from "../../api/types/notices";
import { inTab, kindWord, pathOf, rows, said, unreadCount, waiting } from "./notices";

const notice = (extra: Partial<Notice> = {}): Notice => ({
  id: 1,
  kind: "mentioned",
  by: "lea",
  card_id: 7,
  inbox_item_id: null,
  title: "Fix it",
  body: "@tom look",
  at: 1,
  read: false,
  ...extra,
});

const snapshot = (cardIds: number[], items: number[] = []): Snapshot =>
  ({
    boards: [{ repository: { id: 3 }, cards: cardIds.map((id) => ({ id })) } as unknown as Board],
    harness_changes: [],
    inbox: items.map((id) => ({ id })),
  }) as unknown as Snapshot;

describe("notices", () => {
  it("counts the unread ones", () => {
    expect(unreadCount([notice(), notice({ read: true }), notice({ id: 3 })])).toBe(2);
    expect(unreadCount([])).toBe(0);
  });

  it("says what each kind is, with who caused it", () => {
    expect(said(notice())).toBe("@lea mentioned you: @tom look");
    expect(said(notice({ by: null }))).toBe("Someone mentioned you: @tom look");
    expect(said(notice({ kind: "assigned", body: "Given to you" }))).toBe("Given to you");
    expect(said(notice({ kind: "changed", body: "Moved from Backlog to Done" }))).toBe(
      "Moved from Backlog to Done",
    );
    expect(
      [notice(), notice({ kind: "assigned" }), notice({ kind: "changed" })].map(kindWord),
    ).toEqual(["Mention", "Assigned", "Following"]);
  });

  it("leads to the card on its board, else to the inbox item, else nowhere", () => {
    expect(pathOf(snapshot([7]), notice())).toBe("/board/3?card=7");
    expect(pathOf(snapshot([]), notice({ card_id: 7, inbox_item_id: 4 }))).toBeNull();
    expect(pathOf(snapshot([], [4]), notice({ card_id: null, inbox_item_id: 4 }))).toBe(
      "/inbox?item=4",
    );
    expect(pathOf(snapshot([], []), notice({ card_id: null, inbox_item_id: 4 }))).toBeNull();
  });

  it("shows only the notices of a tab's own kind", () => {
    const all = [
      notice({ id: 1 }),
      notice({ id: 2, kind: "assigned" }),
      notice({ id: 3, kind: "changed" }),
      notice({ id: 4, kind: "changed" }),
    ];
    expect(inTab(all, "mentions").map((each) => each.id)).toEqual([1]);
    expect(inTab(all, "assigned").map((each) => each.id)).toEqual([2]);
    expect(inTab(all, "following").map((each) => each.id)).toEqual([3, 4]);
  });

  it("counts what waits as items and unread notices that are not following", () => {
    const all = [
      notice({ id: 1 }),
      notice({ id: 2, kind: "assigned" }),
      notice({ id: 3, kind: "assigned", read: true }),
      notice({ id: 4, kind: "changed" }),
    ];
    expect(waiting(58, all)).toBe(60);
    expect(waiting(0, [])).toBe(0);
  });

  it("says a capped count with a plus", () => {
    expect(rows(200, true)).toBe("200+");
    expect(rows(12, false)).toBe("12");
  });
});
