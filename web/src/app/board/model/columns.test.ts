/**
 * Tests of the columns of a board, the cards each shows after the filters, and the card the dock shows.
 */
import { describe, expect, it } from "vitest";
import { board, card, item, run, running } from "./fixtures";
import { boardColumns, DONE_SHOWN, groupByColumn, NO_FILTERS } from "./index";

describe("boardColumns", () => {
  it("lists the default workflow, with Backlog even while it holds no card", () => {
    expect(boardColumns(board([card(1)]))).toEqual([
      "Backlog",
      "Not started",
      "In progress",
      "In review",
      "Ready to merge",
      "Done",
    ]);
    expect(boardColumns(board([card(1, { column: "Backlog" })]))[0]).toBe("Backlog");
  });

  it("adds a column of the cards that the workflow does not have, last", () => {
    const columns = boardColumns(board([card(1, { column: "QA" })]));
    expect(columns.at(-1)).toBe("QA");
  });
});

describe("groupByColumn", () => {
  it("orders cards by number and Done newest first, cut to ten", () => {
    const done = Array.from({ length: 12 }, (_, i) => card(100 + i, { column: "Done" }));
    const groups = groupByColumn(board([card(3), card(2), ...done]), [], NO_FILTERS);
    expect(groups.find((g) => g.column === "Not started")?.cards.map((c) => c.number)).toEqual([
      2, 3,
    ]);
    const shown = groups.find((g) => g.column === "Done");
    expect(shown?.cards).toHaveLength(DONE_SHOWN);
    expect(shown?.cards[0]?.number).toBe(111);
    expect(shown?.total).toBe(12);
  });

  it("filters by column, agent and status together", () => {
    const cards = [
      card(1, { column: "In progress", run }),
      card(2, {
        column: "In progress",
        run: { ...running, agent: "Reviewer", status: "succeeded" },
      }),
      card(3, { column: "Not started", blocked_by: [1] }),
    ];
    const b = board(cards);
    const numbers = (filters: Partial<typeof NO_FILTERS>) =>
      groupByColumn(b, [], { ...NO_FILTERS, ...filters }).flatMap((g) =>
        g.cards.map((c) => c.number),
      );
    expect(numbers({ column: "In progress" })).toEqual([1, 2]);
    expect(groupByColumn(b, [], { ...NO_FILTERS, column: "Not started" })).toHaveLength(1);
    expect(numbers({ agent: "Reviewer" })).toEqual([2]);
    expect(numbers({ status: "live" })).toEqual([1]);
    expect(numbers({ status: "blocked" })).toEqual([3]);
    expect(numbers({ status: "held" })).toEqual([]);
    expect(numbers({ status: "live", agent: "Reviewer" })).toEqual([]);
  });

  it("filters by what waits for a person", () => {
    const b = board([card(1), card(2), card(3)]);
    const inbox = [item("approve_issue", 1), item("stuck", 2)];
    const numbers = (status: string) =>
      groupByColumn(b, inbox, { ...NO_FILTERS, status }).flatMap((g) =>
        g.cards.map((c) => c.number),
      );
    expect(numbers("needs-you")).toEqual([1]);
    expect(numbers("stuck")).toEqual([2]);
    expect(numbers("idle")).toEqual([3]);
  });
});
