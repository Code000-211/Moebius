/**
 * Tests of the stacks of sub-issues: parents, children, closed issues and the card started next.
 */
import { describe, expect, it } from "vitest";
import type { Card } from "../../../api/types/board";
import { board, card, run } from "./fixtures";
import {
  descendants,
  groupByColumn,
  isClosed,
  labelsOf,
  NO_FILTERS,
  nextCard,
  parentOf,
  rootOf,
  type Stack,
  stackWords,
} from "./index";

describe("stacks of sub-issues", () => {
  // Issue 1 is a plan in Not started with the sub-issues 2 (in progress), 3 (done) and 4 (whose own sub-issue is 5).
  const stacked = () =>
    board([
      card(1, { column: "Not started" }),
      card(2, { column: "In progress", parent: 1, run, labels: ["spike"] }),
      card(3, { column: "Done", parent: 1 }),
      card(4, { column: "Not started", parent: 1, labels: ["decision"] }),
      card(5, { column: "Backlog", parent: 4 }),
      card(6, { column: "Not started" }),
      // A sub-issue whose parent is not on the board is a card like any other.
      card(7, { column: "Not started", parent: 99 }),
    ]);

  it("lays a plan and everything below it as one stack in the plan's column", () => {
    const groups = groupByColumn(stacked(), [], NO_FILTERS);
    const numbers = (column: string) =>
      groups.find((g) => g.column === column)?.cards.map((c) => c.number);
    expect(numbers("Not started")).toEqual([1, 6, 7]);
    expect(numbers("In progress")).toEqual([]);
    expect(numbers("Done")).toEqual([]);
    const stack = groups[0]?.stacks.get(1);
    expect(stack?.shown.map((c) => c.number)).toEqual([2, 3, 4, 5]);
    expect([stack?.total, stack?.done]).toEqual([4, 1]);
    expect(stackWords(stack as Stack)).toBe("1 of 4 sub-issues done");
    expect(groups[0]?.stacks.has(6)).toBe(false);
  });

  it("applies the filters to the sub-issues, and keeps a plan for the sub-issues that pass", () => {
    const b = stacked();
    const shown = (filters: Partial<typeof NO_FILTERS>) => {
      const groups = groupByColumn(b, [], { ...NO_FILTERS, ...filters });
      return groups.flatMap((g) =>
        g.cards.map((c) => [c.number, ...(g.stacks.get(c.id)?.shown.map((x) => x.number) ?? [])]),
      );
    };
    // The label of one sub-issue: the plan is shown for it alone, and its count still says how the stack stands.
    expect(shown({ label: "decision" })).toEqual([[1, 4]]);
    expect(
      groupByColumn(b, [], { ...NO_FILTERS, label: "decision" })[0]?.stacks.get(1)?.total,
    ).toBe(4);
    expect(shown({ agent: "Coder" })).toEqual([[1, 2]]);
    expect(shown({ label: "nobody has it" })).toEqual([]);
    // The column filter chooses the columns by the card at the top, and the sub-issues in them by their own.
    expect(shown({ column: "Not started" })).toEqual([[1, 4], [6], [7]]);
    expect(shown({ column: "In progress" })).toEqual([]);
  });

  it("finds the top of a stack and everything below a card, and survives a circle", () => {
    const b = stacked();
    expect(rootOf(b, b.cards[4] as Card).number).toBe(1);
    expect(descendants(b, b.cards[0] as Card).map((c) => c.number)).toEqual([2, 3, 4, 5]);
    expect(parentOf(b, b.cards[4] as Card)?.number).toBe(4);
    expect(parentOf(b, b.cards[6] as Card)).toBeUndefined();
    const circle = board([card(1, { parent: 2 }), card(2, { parent: 1 })]);
    expect(rootOf(circle, circle.cards[0] as Card)).toBeDefined();
    expect(groupByColumn(circle, [], NO_FILTERS)).toBeDefined();
  });

  it("does not offer a plan whose sub-issues are open as the card the board starts next", () => {
    const b = stacked();
    const ready = b.cards.filter((c) => c.column === "Not started" && c.parent === null);
    expect(nextCard(ready, b.cards)).toBe(6);
    const done = board([card(1), card(2, { parent: 1, closed: true }), card(6)]);
    expect(
      nextCard(
        done.cards.filter((c) => c.parent === null),
        done.cards,
      ),
    ).toBe(1);
  });

  it("shows an issue that does not count as a sub-issue as a card of its own, and counts it for nothing", () => {
    // Issue 2 says "Part of #1" but nobody vouched for it: the server says it does not count.
    const b = board([
      card(1, { column: "Not started" }),
      card(2, { column: "Not started", parent: 1, counts_as_sub_issue: false }),
      card(3, { column: "Done", parent: 1 }),
    ]);
    const groups = groupByColumn(b, [], NO_FILTERS);
    const numbers = (column: string) =>
      groups.find((g) => g.column === column)?.cards.map((c) => c.number);
    expect(numbers("Not started")).toEqual([1, 2]);
    const stack = groups[0]?.stacks.get(1);
    expect(stack?.shown.map((c) => c.number)).toEqual([3]);
    expect([stack?.total, stack?.done]).toEqual([1, 1]);
    expect(rootOf(b, b.cards[1] as Card).number).toBe(2);
    expect(parentOf(b, b.cards[1] as Card)).toBeUndefined();
    expect(descendants(b, b.cards[0] as Card).map((c) => c.number)).toEqual([3]);
    // It does not stop its parent from being next either.
    expect(
      nextCard(
        b.cards.filter((c) => c.column === "Not started"),
        b.cards,
      ),
    ).toBe(1);
  });

  it("counts a closed issue as done wherever its card is, and lists the labels of the board", () => {
    expect(isClosed(card(1, { closed: true }))).toBe(true);
    expect(isClosed(card(1, { column: "Done" }))).toBe(true);
    expect(isClosed(card(1))).toBe(false);
    expect(labelsOf(stacked())).toEqual(["decision", "spike"]);
  });
});
