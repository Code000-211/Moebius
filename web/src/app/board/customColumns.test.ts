/**
 * What changes when a project has columns of its own: the board lists them in place, a drop on one means what a drop on
 * the built-in column it counts as means, and every question about "done", "in review" or "in progress" is asked of the
 * class, not of the name.
 */
import { describe, expect, it } from "vitest";
import type { ColumnDef } from "../../api/types/board";
import { groupOf, iconOf } from "../issues/model";
import { byColumn } from "../views/model";
import { bulkCommands, movable } from "./bulkCommands";
import { ColumnMark } from "./ColumnMark";
import { cardCommands } from "./cardCommands";
import { boardColumns, DONE_SHOWN, dropRule, groupByColumn, NO_FILTERS } from "./model";
import { board, card, item } from "./model/fixtures";
import { isClosed } from "./model/stacks";
import { cardDetail, cardIcon } from "./model/words";
import { moveLabel, timelineOf } from "./timeline";
import { listGroups } from "./view";

const COLUMNS: ColumnDef[] = [
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

const noop = () => {};
const actions = {
  claim: noop,
  release: noop,
  close: noop,
  reopen: noop,
  block: noop,
  unblock: noop,
  stop: noop,
  addSubIssue: noop,
  approve: noop,
  retry: noop,
  merge: noop,
  assignTo: noop,
  reject: noop,
};

describe("the board's columns", () => {
  it("are the six built-in ones for a server that sends none", () => {
    expect(boardColumns(board([]))).toHaveLength(6);
  });

  it("lists the custom ones where the project puts them, then any column a card is in that the project does not list", () => {
    const b = board([card(1, { column: "Elsewhere" })], { columns: COLUMNS });
    expect(boardColumns(b)).toEqual([...COLUMNS.map((c) => c.name), "Elsewhere"]);
  });

  it("gives each column its cards, and lists only some of a custom column that counts as Done", () => {
    const many = Array.from({ length: DONE_SHOWN + 3 }, (_, i) =>
      card(i + 1, { column: "Signed off" }),
    );
    const b = board([card(100, { column: "Ready for testing" }), ...many], { columns: COLUMNS });
    const lanes = groupByColumn(b, [], NO_FILTERS);
    expect(lanes.map((l) => l.column)).toEqual(COLUMNS.map((c) => c.name));
    const testing = lanes.find((l) => l.column === "Ready for testing");
    expect(testing?.cards.map((c) => c.id)).toEqual([100]);
    const signed = lanes.find((l) => l.column === "Signed off");
    expect(signed?.cards).toHaveLength(DONE_SHOWN);
    expect(signed?.total).toBe(DONE_SHOWN + 3);
    // Done lists the newest first.
    expect(signed?.cards[0]?.id).toBe(DONE_SHOWN + 3);
  });

  it("filters by a custom column", () => {
    const b = board([card(1, { column: "Being measured" }), card(2)], { columns: COLUMNS });
    const lanes = groupByColumn(b, [], { ...NO_FILTERS, column: "Being measured" });
    expect(lanes.map((l) => l.column)).toEqual(["Being measured"]);
  });

  it("groups the list layout by the custom columns in order", () => {
    const b = board([card(1, { column: "Being measured" }), card(2, { column: "In review" })], {
      columns: COLUMNS,
    });
    expect(listGroups(b, [], NO_FILTERS, "", "").map((g) => g.name)).toEqual([
      "In review",
      "Being measured",
    ]);
  });
});

describe("a drop on a custom column", () => {
  it("means what a drop on its class means", () => {
    const merge = item("ready_to_merge", 1, { change: 4 });
    const ready = card(1, { column: "Ready to merge" });
    expect(dropRule(ready, "Signed off", merge, COLUMNS)).toEqual({ kind: "merge", change: 4 });
    expect(dropRule(ready, "Ready for testing", merge, COLUMNS)).toMatchObject({
      kind: "refused",
    });
    expect(
      dropRule(card(2, { column: "Backlog" }), "Not started", item("approve_issue", 2), COLUMNS),
    ).toEqual({ kind: "approve" });
  });

  it("refuses a card nobody decides, naming the column it is in", () => {
    const drop = dropRule(card(1, { column: "Being measured" }), "Done", undefined, COLUMNS);
    expect(drop).toMatchObject({ kind: "refused" });
    expect(drop.kind === "refused" && drop.reason).toContain("Being measured");
  });
});

describe("the commands that move a card", () => {
  it("offers a move to each column of the project that a person may decide", () => {
    const ready = card(1, { column: "Ready to merge" });
    const commands = cardCommands(
      ready,
      item("ready_to_merge", 1, { change: 4 }),
      actions,
      [],
      COLUMNS,
    );
    expect(commands.map((c) => c.id)).toContain("move-Signed off");
    expect(commands.map((c) => c.id)).not.toContain("move-Ready for testing");
  });

  it("moves the cards a person decides for in bulk", () => {
    const ready = card(1, { column: "Ready to merge" });
    const merge = item("ready_to_merge", 1, { change: 4 });
    expect(movable([ready], "Signed off", () => merge, COLUMNS)).toEqual([ready]);
    const commands = bulkCommands(
      [ready],
      () => merge,
      {
        claim: noop,
        release: noop,
        block: noop,
        unblock: noop,
        stop: noop,
        move: noop,
      },
      COLUMNS,
    );
    expect(commands.map((c) => c.id)).toContain("move-Signed off");
  });
});

describe("what a card in a custom column is", () => {
  it("is closed in a column that counts as Done", () => {
    expect(isClosed(card(1, { column: "Signed off", column_class: "Done" }))).toBe(true);
    expect(isClosed(card(1, { column: "Being measured", column_class: "In review" }))).toBe(false);
  });

  it("wears the icon and words of its class", () => {
    const measured = card(1, { column: "Being measured", column_class: "In review", rounds: 2 });
    expect(cardIcon(measured, "idle")).toBe("inReview");
    expect(cardDetail(measured, 5)).toBe("round 2 of 5");
    expect(groupOf(measured)).toBe("In review");
    expect(iconOf(measured)).toBe("inReview");
    expect(groupOf(card(2, { column: "Signed off", column_class: "Done" }))).toBe("Done");
  });

  it("has the mark of its class, by the class the server sent or by the project's columns", () => {
    expect(ColumnMark({ name: "Signed off", counts: "Done" }).props.state).toBe("done");
    expect(ColumnMark({ name: "Being measured", columns: COLUMNS }).props.state).toBe("inReview");
    expect(ColumnMark({ name: "Mystery" }).props.state).toBe("notStarted");
  });
});

describe("the timeline", () => {
  it("says a card was sent back when it moved from a custom column of In review to one of In progress", () => {
    const moved = {
      id: 1,
      card_id: 1,
      at: 1,
      kind: "moved",
      data: { from: "Being measured", to: "In progress", note: "needs more work" },
    };
    const [entry] = timelineOf([moved as never], [], undefined, COLUMNS);
    expect(entry?.type === "move" && moveLabel(entry.event)).toBe("Sent back to In progress");
  });
});

describe("a view that spans projects", () => {
  it("sets the custom columns after the last built-in column of their class", () => {
    const cards = [
      { ...card(1, { column: "Done" }), cycle: null },
      { ...card(2, { column: "Being measured", column_class: "In review" }), cycle: null },
      { ...card(3, { column: "In review" }), cycle: null },
      { ...card(4, { column: "Ready to merge" }), cycle: null },
    ];
    expect(byColumn(cards).map((g) => g.name)).toEqual([
      "In review",
      "Being measured",
      "Ready to merge",
      "Done",
    ]);
  });
});
