import { describe, expect, it } from "vitest";
import type { Card } from "../../api/types/board";
import type { Cycle } from "../../api/types/cycles";
import type { View, ViewCard, ViewFilter } from "../../api/types/views";
import { NO_FILTERS } from "../board/model";
import {
  boardFiltersOf,
  boardOfView,
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
  it("keeps the repository, every filter, the layout and the sort", () => {
    const fields = viewOfBoard(
      "View 1",
      7,
      {
        column: "Backlog",
        agent: "coder",
        status: "held",
        label: "bug",
        priority: "urgent",
        due: "overdue",
      },
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
        agents: ["coder"],
        statuses: ["held"],
        labels: ["bug"],
        priorities: ["urgent"],
        due: "overdue",
      },
    });
  });

  it("leaves a filter that is not set out of the view", () => {
    const fields = viewOfBoard("View 2", 1, NO_FILTERS, "board", "due");
    expect(fields.filter).toEqual({
      repositories: [1],
      columns: [],
      agents: [],
      statuses: [],
      labels: [],
      priorities: [],
      due: "",
    });
    expect(fields.sort).toBe("due");
  });
});

/** Every repository is at project 2, except 9, which no project holds. */
const home = (repository: number) => (repository === 9 ? undefined : 2);

describe("a saved view opened on the board", () => {
  const made = (changes: Partial<View>): View => ({
    id: 4,
    name: "Mine",
    owner_id: 1,
    shared: false,
    layout: "board",
    sort: "created",
    descending: false,
    created_at: 1,
    can_change: true,
    ...changes,
    filter: { ...NO_FILTER, repositories: [7], ...changes.filter },
  });

  it("is the board of its repository with the view's filter, sort and name in the address", () => {
    const view = made({
      sort: "created",
      descending: true,
      filter: {
        ...NO_FILTER,
        repositories: [7],
        columns: ["Backlog"],
        agents: ["coder"],
        statuses: ["needs-you"],
        labels: ["bug"],
        priorities: ["urgent"],
        due: "week",
      },
    });
    const path = boardOfView(view, home) as string;
    const [pathname, query] = path.split("?");
    expect(pathname).toBe("/projects/2");
    expect(Object.fromEntries(new URLSearchParams(query))).toEqual({
      repository: "7",
      column: "Backlog",
      agent: "coder",
      status: "needs-you",
      label: "bug",
      priority: "urgent",
      due: "week",
      sort: "newest",
      view: "4",
      card: "none",
    });
    expect(boardFiltersOf(view)?.filters.status).toBe("needs-you");
  });

  it("keeps the list layout of the board, and no sort that is the default", () => {
    expect(boardOfView(made({ layout: "list" }), home)).toBe(
      "/projects/2?repository=7&layout=list&view=4&card=none",
    );
  });

  it("stays on the Views page when no project holds the repository", () => {
    expect(boardOfView(made({ filter: { ...NO_FILTER, repositories: [9] } }), home)).toBeNull();
  });

  it("stays on the Views page when the board cannot say what the view asks", () => {
    const filter = (changes: Partial<ViewFilter>) => made({ filter: { ...NO_FILTER, ...changes } });
    expect(boardOfView(made({ layout: "calendar" }), home)).toBeNull();
    expect(boardOfView(made({ layout: "timeline" }), home)).toBeNull();
    expect(boardOfView(filter({ repositories: [] }), home)).toBeNull();
    expect(boardOfView(filter({ repositories: [7, 8] }), home)).toBeNull();
    expect(boardOfView(filter({ repositories: [7], agents: ["a", "b"] }), home)).toBeNull();
    expect(boardOfView(filter({ repositories: [7], statuses: ["idle", "live"] }), home)).toBeNull();
    expect(boardOfView(filter({ repositories: [7], assignees: ["lea"] }), home)).toBeNull();
    expect(boardOfView(filter({ repositories: [7], authors: ["lea"] }), home)).toBeNull();
    expect(boardOfView(filter({ repositories: [7], text: "login" }), home)).toBeNull();
    expect(boardOfView(filter({ repositories: [7], cycles: [1] }), home)).toBeNull();
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
        agents: ["coder"],
        statuses: ["live", "held"],
        due: "overdue",
        text: "login",
      }),
    ).toBe(
      'label bug · priority urgent, high · cycle 3 · agent coder · status live, held · due overdue · text "login"',
    );
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
