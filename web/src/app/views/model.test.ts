import { describe, expect, it } from "vitest";
import type { Card } from "../../api/types/board";
import type { Cycle } from "../../api/types/cycles";
import type { ViewCard } from "../../api/types/views";
import { NO_FILTERS } from "../board/model";
import {
  byColumn,
  byCycle,
  byDueDate,
  filterWords,
  groupsOf,
  NO_FILTER,
  viewOfBoard,
} from "./model";

const cycle = (id: number, name: string, starts_on: string): Cycle => ({
  id,
  project_id: 1,
  name,
  starts_on,
  ends_on: "2027-01-31",
  closed_at: null,
  created_at: 1,
});

function card(id: number, column: string, due: string | null, in_cycle: Cycle | null): ViewCard {
  return { id, column, due, cycle: in_cycle, title: `Card ${id}` } as unknown as Card & {
    cycle: Cycle | null;
  };
}

describe("a board saved as a view", () => {
  it("keeps the repository, the column, label and priority filters, the layout and the sort", () => {
    const { fields, dropped } = viewOfBoard(
      "View 1",
      7,
      { ...NO_FILTERS, column: "Backlog", label: "bug", priority: "urgent" },
      "list",
      "newest",
    );
    expect(fields).toEqual({
      name: "View 1",
      shared: false,
      layout: "list",
      sort: "created",
      descending: true,
      filter: {
        repositories: [7],
        columns: ["Backlog"],
        labels: ["bug"],
        priorities: ["urgent"],
      },
    });
    expect(dropped).toEqual([]);
  });

  it("says which filters a view cannot keep", () => {
    const { dropped, fields } = viewOfBoard(
      "View 2",
      1,
      { ...NO_FILTERS, agent: "Coder", status: "held", due: "overdue" },
      "board",
      "due",
    );
    expect(dropped).toEqual(["agent", "status", "due date"]);
    expect(fields.filter).toEqual({ repositories: [1], columns: [], labels: [], priorities: [] });
    expect(fields.sort).toBe("due");
  });
});

describe("a filter in words", () => {
  it("names every part that is set", () => {
    expect(filterWords(NO_FILTER)).toBe("everything");
    expect(
      filterWords({
        ...NO_FILTER,
        labels: ["bug"],
        priorities: ["urgent", "high"],
        cycles: [3],
        text: "login",
      }),
    ).toBe('label bug · priority urgent, high · cycle 3 · text "login"');
  });
});

describe("the layouts", () => {
  const one = cycle(1, "Cycle 1", "2026-11-02");
  const two = cycle(2, "Cycle 2", "2026-11-16");
  const cards = [
    card(1, "Done", "2026-12-02", two),
    card(2, "Backlog", null, null),
    card(3, "In progress", "2026-12-01", one),
    card(4, "Odd lane", "2026-12-01", null),
  ];

  it("sets the board in the usual lanes first and the others after", () => {
    expect(byColumn(cards).map((g) => g.name)).toEqual([
      "Backlog",
      "In progress",
      "Done",
      "Odd lane",
    ]);
  });

  it("sets the calendar by due day, the earliest first, and the undated last", () => {
    const groups = byDueDate(cards);
    expect(groups.map((g) => g.name)).toEqual(["2026-12-01", "2026-12-02", "No due date"]);
    expect(groups[0]?.cards.map((c) => c.id)).toEqual([3, 4]);
  });

  it("sets the timeline by cycle, the earliest start first, and the cycleless last", () => {
    expect(byCycle(cards).map((g) => g.name)).toEqual([
      "Cycle 1 · 2026-11-02 to 2027-01-31",
      "Cycle 2 · 2026-11-16 to 2027-01-31",
      "No cycle",
    ]);
  });

  it("has no headings in the list", () => {
    const groups = groupsOf("list", cards);
    expect(groups).toHaveLength(1);
    expect(groups[0]?.name).toBe("");
    expect(groupsOf("calendar", cards)).toEqual(byDueDate(cards));
  });
});
