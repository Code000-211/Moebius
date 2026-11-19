/**
 * Tests of what a drop of a card on a column means.
 */
import { describe, expect, it } from "vitest";
import { card, held, item, run } from "./fixtures";
import { dropRule } from "./index";

describe("dropRule", () => {
  it("merges a change dropped on Done and sends it back when dropped on In progress", () => {
    const c = card(1, { column: "Ready to merge" });
    const waiting = item("ready_to_merge", 1, { change: 9 });
    expect(dropRule(c, "Done", waiting)).toEqual({ kind: "merge", change: 9 });
    expect(dropRule(c, "In progress", waiting)).toEqual({ kind: "reject", change: 9 });
  });

  it("refuses a change dropped anywhere else, and says what is possible", () => {
    const c = card(1, { column: "Ready to merge" });
    const drop = dropRule(c, "Not started", item("ready_to_merge", 1, { change: 9 }));
    expect(drop).toMatchObject({ kind: "refused" });
    expect(drop.kind === "refused" && drop.reason).toContain("drop it on Done");
  });

  it("refuses to merge a card whose item names no change", () => {
    const drop = dropRule(card(1), "Done", item("ready_to_merge", 1));
    expect(drop).toMatchObject({ kind: "refused" });
  });

  it("approves an issue dropped on Not started", () => {
    const c = card(1, { column: "Backlog" });
    expect(dropRule(c, "Not started", item("approve_issue", 1))).toEqual({ kind: "approve" });
    expect(dropRule(c, "Done", item("approve_issue", 1))).toMatchObject({ kind: "refused" });
  });

  it("retries a stuck card or one with a closed pull request when dropped on In progress", () => {
    expect(dropRule(card(1, { column: "In progress" }), "In progress", item("stuck", 1))).toEqual({
      kind: "retry",
    });
    expect(
      dropRule(card(1, { column: "Ready to merge" }), "In progress", item("change_closed", 1)),
    ).toEqual({
      kind: "retry",
    });
    expect(dropRule(card(1, { column: "In progress" }), "Done", item("stuck", 1))).toMatchObject({
      kind: "refused",
    });
  });

  it("refuses to move a card that is working", () => {
    const drop = dropRule(card(1, { column: "In progress", run }), "Done", undefined);
    expect(drop.kind === "refused" && drop.reason).toContain("working on this card");
  });

  it("refuses any other move with the reason that the board moves it, and ignores a drop on its own column", () => {
    const c = card(1, { column: "Not started" });
    const drop = dropRule(c, "Done", undefined);
    expect(drop.kind === "refused" && drop.reason).toContain("script");
    expect(dropRule(c, "Not started", undefined)).toEqual({ kind: "none" });
  });

  it("says a blocked card moves again once it is unblocked", () => {
    const drop = dropRule(card(1, { column: "Not started", held }), "Done", undefined);
    expect(drop.kind === "refused" && drop.reason).toContain("Unblock it");
    // What waits for a person is still theirs to decide while the card is blocked.
    const waiting = item("approve_issue", 1);
    expect(dropRule(card(1, { column: "Backlog", held }), "Not started", waiting)).toEqual({
      kind: "approve",
    });
  });
});
