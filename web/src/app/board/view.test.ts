import { describe, expect, it } from "vitest";
import type { Board, Card } from "../../api/types/board";
import { groupByColumn, NO_FILTERS, orderCards } from "./model";
import { filterFields, listGroups } from "./view";

function card(id: number, extra: Partial<Card> = {}): Card {
  return {
    id,
    repository_id: 1,
    number: id,
    title: `Card ${id}`,
    body: "",
    author: "person",
    column: "Not started",
    created_at: id,
    note: null,
    retried_after: null,
    rounds: 0,
    tokens: 0,
    run: null,
    blocked_by: [],
    refusals: 0,
    change: null,
    parent: null,
    counts_as_sub_issue: true,
    closed: false,
    labels: [],
    claimed_by: null,
    ...extra,
  };
}

function board(cards: Card[]): Board {
  return {
    repository: {
      id: 1,
      name: "demo",
      path: "/r",
      forge: "github",
      default_branch: "main",
      github: "acme/demo",
      trusted: [],
    },
    cards,
    round_limit: 5,
    waiting: {},
    changes: [],
  };
}

const run = { id: 1, agent: "Coder", purpose: "code", status: "running" as const };

const cards = [
  card(1, { column: "Backlog", title: "Zeta", labels: ["spike"] }),
  card(2, { column: "In progress", title: "Alpha", run, labels: ["spike", "ui"] }),
  card(3, { column: "Done", title: "Mid", created_at: 30, closed: true }),
  card(4, { column: "Done", title: "Beta", created_at: 10, parent: 2 }),
];

describe("the fields of the filter bar", () => {
  it("offers the four filters the selects had, with their values", () => {
    const fields = filterFields(board(cards), NO_FILTERS);
    expect(fields.map((f) => f.key)).toEqual(["column", "agent", "status", "label"]);
    expect(fields[1]?.options).toEqual([["Coder", "Coder"]]);
    expect(fields[3]?.options).toEqual([
      ["spike", "spike"],
      ["ui", "ui"],
    ]);
  });

  it("keeps a label that is set although no card carries it any more", () => {
    const fields = filterFields(board(cards), { ...NO_FILTERS, label: "gone" });
    expect(fields[3]?.options.map(([v]) => v)).toContain("gone");
  });
});

describe("sorting", () => {
  it("orders by number, with Done newest first, newest first, or by title", () => {
    const numbers = (sort: string, column?: string) =>
      orderCards(cards, sort, column).map((c) => c.number);
    expect(numbers("")).toEqual([1, 2, 3, 4]);
    expect(numbers("", "Done")).toEqual([4, 3, 2, 1]);
    expect(numbers("newest")).toEqual([3, 4, 2, 1]);
    expect(numbers("title")).toEqual([2, 4, 3, 1]);
  });

  it("sorts the cards of each lane of the board", () => {
    const lane = groupByColumn(
      board([...cards, card(5, { column: "Done", title: "Aaa" })]),
      [],
      NO_FILTERS,
      "title",
    ).find((g) => g.column === "Done");
    expect(lane?.cards.map((c) => c.number)).toEqual([5, 3]);
  });
});

describe("the list layout", () => {
  const names = (group: string, sort = "", filters = NO_FILTERS) =>
    listGroups(board(cards), [], filters, group, sort).map((g) => [
      g.name,
      g.cards.map((c) => c.number),
    ]);

  it("groups by column and leaves out an empty one", () => {
    expect(names("")).toEqual([
      ["Backlog", [1]],
      ["In progress", [2]],
      ["Done", [4, 3]],
    ]);
  });

  it("draws a sub-issue as a row of its own, and filters it like any card", () => {
    expect(names("none", "", { ...NO_FILTERS, column: "Done" })).toEqual([["Cards", [3, 4]]]);
  });

  it("groups by status, agent and label", () => {
    expect(names("status")).toEqual([
      ["Live", [2]],
      ["Idle", [1, 3, 4]],
    ]);
    expect(names("agent")).toEqual([
      ["Coder", [2]],
      ["No agent", [1, 3, 4]],
    ]);
    // A card with two labels is under both.
    expect(names("label")).toEqual([
      ["spike", [1, 2]],
      ["ui", [2]],
      ["No label", [3, 4]],
    ]);
  });

  it("sorts inside the groups", () => {
    expect(names("none", "title")).toEqual([["Cards", [2, 4, 3, 1]]]);
  });
});
