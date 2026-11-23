/**
 * Tests of the state of a card, the open item of the inbox and the changes of a card over several repositories.
 */
import { describe, expect, it } from "vitest";
import { board, card, held, item, run } from "./fixtures";
import {
  cardBadges,
  cardStatus,
  changesOfCard,
  inboxOf,
  mergeStopped,
  mergeStoppedOf,
} from "./index";

describe("cardStatus", () => {
  it("takes the first that applies", () => {
    const b = board([card(1)], { waiting: { "1": "waiting for CI" } });
    expect(cardStatus(card(1, { run, held }), item("stuck", 1), b)).toBe("held");
    expect(cardStatus(card(1, { run }), item("stuck", 1), b)).toBe("live");
    expect(cardStatus(card(1), item("stuck", 1), b)).toBe("stuck");
    expect(cardStatus(card(1), item("ready_to_merge", 1), b)).toBe("needs-you");
    expect(cardStatus(card(1, { blocked_by: [2] }), undefined, b)).toBe("blocked");
    expect(cardStatus(card(1), undefined, b)).toBe("waiting");
    expect(cardStatus(card(2), undefined, b)).toBe("idle");
  });

  it("ignores a resolved inbox item", () => {
    const resolved = { ...item("stuck", 1), resolved_at: 5 };
    expect(inboxOf([resolved], card(1))).toBeUndefined();
  });
});

describe("the changes of a card over several repositories", () => {
  const own = {
    id: 1,
    repository: "demo",
    state: "open",
    checks: "success" as const,
    review_verdict: null,
  };
  const web = {
    id: 2,
    repository: "web",
    state: "open",
    checks: "failure" as const,
    review_verdict: null,
  };

  it("lists every open change, and the one change of a server that has no list", () => {
    expect(changesOfCard(card(1, { change: own, changes: [own, web] }))).toEqual([own, web]);
    expect(changesOfCard(card(1, { change: own }))).toEqual([own]);
    expect(changesOfCard(card(1))).toEqual([]);
  });

  it("badges the checks of each change, naming its repository only when there are several", () => {
    const both = card(1, { change: own, changes: [own, web] });
    expect(cardBadges(both, "idle", false).map((badge) => badge.name)).toEqual([
      "Green in demo",
      "CI failed in web",
    ]);
    const single = card(1, { change: own, changes: [own] });
    expect(cardBadges(single, "idle", false).map((badge) => badge.name)).toEqual(["Green"]);
  });

  it("gives a card with several changes to merge the first, which the merge order allows", () => {
    const ready = (id: number, change: number) => ({
      ...item("ready_to_merge", 1),
      id,
      data: { change },
    });
    expect(inboxOf([ready(8, 2), ready(7, 1)], card(1))?.id).toBe(7);
  });

  it("lets the rounds spent be the card's item without hiding the choices of its pull request", () => {
    const outside = { ...item("outside_change", 1), id: 3, data: { change: 9 } };
    const spent = { ...item("review_rounds_spent", 1), id: 4, data: { change: 9 } };
    // Whatever order the inbox lists them in, the card has one item, which carries the change to review and merge.
    expect(inboxOf([outside, spent], card(1))?.kind).toBe("review_rounds_spent");
    expect(inboxOf([spent, outside], card(1))?.kind).toBe("review_rounds_spent");
    expect(inboxOf([spent, outside], card(1))?.data.change).toBe(9);
  });

  it("leaves a stopped merge out of the card's turn, and finds it for what it says", () => {
    const stopped = {
      ...item("merge_stopped", 1),
      id: 9,
      data: { repository: "web", reason: "no longer merges", merged: ["demo"], open: ["web"] },
    };
    const ready = { ...item("ready_to_merge", 1), id: 8 };
    expect(inboxOf([stopped, ready], card(1))?.id).toBe(8);
    expect(inboxOf([ready, stopped], card(1))?.id).toBe(8);
    expect(inboxOf([stopped], card(1))?.id).toBe(9);
    expect(mergeStoppedOf([ready, stopped], card(1))?.id).toBe(9);
    expect(mergeStoppedOf([{ ...stopped, resolved_at: 5 }], card(1))).toBeUndefined();
    expect(mergeStopped(stopped)).toEqual({
      repository: "web",
      reason: "no longer merges",
      merged: ["demo"],
      open: ["web"],
    });
  });
});
