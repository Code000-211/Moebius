import { describe, expect, it, vi } from "vitest";
import type { Card, InboxItem } from "../../api/types/board";
import { type BulkActions, bulkCommands } from "./bulkCommands";

function card(id: number, extra: Partial<Card> = {}): Card {
  return {
    id,
    number: id,
    title: `Card ${id}`,
    column: "Backlog",
    claimed_by: null,
    assignees: [],
    priority: "none",
    due: null,
    estimate: null,
    held: null,
    run: null,
    closed: false,
    ...extra,
  } as unknown as Card;
}

const item = (cardId: number, kind: string): InboxItem =>
  ({
    id: cardId,
    card_id: cardId,
    kind,
    data: { change: 7 },
    resolved_at: null,
  }) as unknown as InboxItem;

function acts(): BulkActions {
  return {
    claim: vi.fn(),
    release: vi.fn(),
    block: vi.fn(),
    unblock: vi.fn(),
    stop: vi.fn(),
    move: vi.fn(),
  };
}

describe("bulkCommands", () => {
  it("offers nothing for an empty choice", () => {
    expect(bulkCommands([], () => undefined, acts())).toEqual([]);
  });

  it("assigns the cards nobody has, then unassigns once all are taken", () => {
    const act = acts();
    const some = bulkCommands([card(1), card(2, { claimed_by: "me" })], () => undefined, act);
    const assign = some.find((c) => c.id === "assign");
    expect(assign?.label).toBe("Assign to me (1 of 2)");
    assign?.run();
    expect(act.claim).toHaveBeenCalledWith([expect.objectContaining({ id: 1 })]);
    const all = bulkCommands([card(2, { claimed_by: "me" })], () => undefined, act);
    expect(all.map((c) => c.id)).toContain("unassign");
  });

  it("moves each card by the decision a drop would make, and leaves the others", () => {
    const act = acts();
    const cards = [card(1), card(2), card(3)];
    const items = new Map([
      [1, item(1, "approve_issue")],
      [2, item(2, "approve_issue")],
    ]);
    const commands = bulkCommands(cards, (c) => items.get(c.id), act);
    const move = commands.find((c) => c.id === "move-Not started");
    expect(move?.label).toBe("Move to Not started (2 of 3)");
    expect(move?.group).toBe("Move");
    move?.run();
    expect(act.move).toHaveBeenCalledWith(
      [expect.objectContaining({ id: 1 }), expect.objectContaining({ id: 2 })],
      "Not started",
    );
  });

  it("does not offer a move that asks for a reason (sending a change back)", () => {
    const commands = bulkCommands(
      [card(5, { column: "Ready to merge" })],
      () => item(5, "ready_to_merge"),
      acts(),
    );
    expect(commands.map((c) => c.id)).toContain("move-Done");
    expect(commands.map((c) => c.id)).not.toContain("move-In progress");
  });
});
