import { describe, expect, it } from "vitest";
import {
  BUILT_IN_COLUMNS,
  cardClass,
  classOf,
  columnIndex,
  columnNames,
  columnsOf,
  columnsOfCards,
  isDone,
  unionColumns,
  withClasses,
  withProjectClasses,
} from "./columns";
import type { Card, ColumnDef } from "./types/board";

const CUSTOM: ColumnDef[] = [
  { name: "Backlog", counts_as: "Backlog" },
  { name: "Not started", counts_as: "Not started" },
  { name: "In progress", counts_as: "In progress" },
  { name: "In review", counts_as: "In review" },
  { name: "Ready for testing", counts_as: "In review" },
  { name: "Being measured", counts_as: "In review" },
  { name: "Ready to merge", counts_as: "Ready to merge" },
  { name: "Signed off", counts_as: "Done" },
  { name: "Done", counts_as: "Done" },
];

describe("columnsOf", () => {
  it("is the six built-in columns when the server named none", () => {
    expect(columnNames(undefined)).toEqual([
      "Backlog",
      "Not started",
      "In progress",
      "In review",
      "Ready to merge",
      "Done",
    ]);
    expect(columnsOf({})).toBe(BUILT_IN_COLUMNS);
    expect(columnsOf({ columns: [] })).toBe(BUILT_IN_COLUMNS);
  });

  it("takes the list of a board, a project or the list itself, in the server's order", () => {
    expect(columnNames({ columns: CUSTOM })).toContain("Ready for testing");
    expect(columnNames(CUSTOM)).toEqual(CUSTOM.map((c) => c.name));
  });
});

describe("classOf", () => {
  it("says what a custom column counts as, and a built-in one is its own class", () => {
    expect(classOf({ columns: CUSTOM }, "Ready for testing")).toBe("In review");
    expect(classOf({ columns: CUSTOM }, "Signed off")).toBe("Done");
    expect(classOf(undefined, "Done")).toBe("Done");
  });

  it("counts a column nobody lists as Not started", () => {
    expect(classOf(undefined, "Mine")).toBe("Not started");
  });
});

describe("cardClass and isDone", () => {
  it("prefers the class the card carries", () => {
    expect(cardClass({ column: "Signed off", column_class: "Done" })).toBe("Done");
    expect(isDone({ column: "Signed off", column_class: "Done" })).toBe(true);
  });

  it("works the class out from the columns when the card has none", () => {
    expect(isDone({ column: "Signed off" }, CUSTOM)).toBe(true);
    expect(isDone({ column: "Ready for testing" }, CUSTOM)).toBe(false);
    expect(isDone({ column: "Done" })).toBe(true);
  });
});

describe("columnIndex", () => {
  it("orders by the project's columns, unknown names last", () => {
    expect(columnIndex(CUSTOM, "Being measured")).toBe(5);
    expect(columnIndex(CUSTOM, "Nowhere")).toBe(CUSTOM.length);
  });
});

describe("unionColumns", () => {
  it("keeps the built-in order and puts a custom column after the one before it at home", () => {
    const names = unionColumns([CUSTOM]).map((c) => c.name);
    expect(names).toEqual(CUSTOM.map((c) => c.name));
  });

  it("merges the columns of several projects once each, in a stable order", () => {
    const other: ColumnDef[] = [
      ...BUILT_IN_COLUMNS.slice(0, 3),
      { name: "Design", counts_as: "In progress" },
      ...BUILT_IN_COLUMNS.slice(3),
    ];
    const names = unionColumns([CUSTOM, other]).map((c) => c.name);
    expect(names).toEqual([
      "Backlog",
      "Not started",
      "In progress",
      "Design",
      "In review",
      "Ready for testing",
      "Being measured",
      "Ready to merge",
      "Signed off",
      "Done",
    ]);
  });
});

describe("columnsOfCards", () => {
  it("puts a custom column after the last column of the class its cards carry", () => {
    const names = columnsOfCards([
      { column: "Zeta", column_class: "In review" },
      { column: "Alpha", column_class: "In review" },
      { column: "Mine" },
    ]).map((c) => c.name);
    expect(names).toEqual([
      "Backlog",
      "Not started",
      "In progress",
      "In review",
      "Alpha",
      "Zeta",
      "Ready to merge",
      "Done",
      "Mine",
    ]);
  });
});

describe("withClasses", () => {
  it("gives each card the class of its column", () => {
    const card = { id: 1, column: "Signed off" } as Card;
    const board = withClasses({ cards: [card], columns: CUSTOM });
    expect(board.cards[0]?.column_class).toBe("Done");
  });

  it("leaves a board whose cards have one as it is", () => {
    const board = { cards: [{ id: 1, column: "Done", column_class: "Done" } as Card] };
    expect(withClasses(board)).toBe(board);
  });
});

describe("withClasses of a board of several projects", () => {
  it("takes the columns of the projects as one list", () => {
    const board = withClasses({
      cards: [{ id: 1, column: "Ready for testing" } as Card],
      project_columns: { "1": [...CUSTOM], "2": [...BUILT_IN_COLUMNS] },
    });
    expect(columnNames(board as { columns?: ColumnDef[] })).toEqual(CUSTOM.map((c) => c.name));
    expect(board.cards[0]?.column_class).toBe("In review");
  });
});

describe("withProjectClasses", () => {
  const cards = [
    { column: "Being measured", project_id: 1 },
    { column: "Being measured", project_id: 2 },
    { column: "Done", project_id: 1, column_class: "Done" as const },
    { column: "Signed off", project_id: 1, column_class: "Ready to merge" as const },
  ];

  it("gives a card without a class the one its column has in its own project, and keeps one that came", () => {
    const projects = [{ id: 1, columns: CUSTOM }, { id: 2 }];
    expect(withProjectClasses(cards, projects).map((card) => card.column_class)).toEqual([
      "In review",
      "Not started",
      "Done",
      "Ready to merge",
    ]);
  });

  it("falls back to Not started for a project it does not know", () => {
    expect(withProjectClasses(cards.slice(0, 1), [])[0]?.column_class).toBe("Not started");
  });
});
