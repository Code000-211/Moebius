import { describe, expect, it } from "vitest";
import type { Board, Card, Change, InboxItem } from "../../api/types/board";
import {
  agentsOf,
  boardColumns,
  cardBadges,
  cardDetail,
  cardStatus,
  changeOf,
  DONE_SHOWN,
  describeHistory,
  dropRule,
  groupByColumn,
  inboxOf,
  inspectedCard,
  issueUrl,
  NO_FILTERS,
  nextCard,
  pullUrl,
  tokens,
  waitingLine,
} from "./model";

function card(id: number, extra: Partial<Card> = {}): Card {
  return {
    id,
    repository_id: 1,
    number: id,
    title: `Card ${id}`,
    body: "",
    author: "person",
    column: "Not started",
    created_at: 0,
    note: null,
    retried_after: null,
    rounds: 0,
    tokens: 0,
    run: null,
    blocked_by: [],
    refusals: 0,
    change: null,
    ...extra,
  };
}

function board(cards: Card[], extra: Partial<Board> = {}): Board {
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
    ...extra,
  };
}

function item(kind: string, cardId: number | null, data: Record<string, unknown> = {}): InboxItem {
  return { id: 1, kind, card_id: cardId, title: kind, data, resolved_at: null, at: 0 };
}

const running = { id: 1, agent: "Coder", purpose: "code", status: "running" as const };

describe("boardColumns", () => {
  it("lists the default workflow, with Backlog only while it holds a card", () => {
    expect(boardColumns(board([card(1)]))).toEqual([
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

const run = running;

describe("cardStatus", () => {
  it("takes the first that applies", () => {
    const b = board([card(1)], { waiting: { "1": "waiting for CI" } });
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

describe("agentsOf", () => {
  it("lists each agent that has a run, sorted", () => {
    const b = board([
      card(1, { run }),
      card(2, { run: { ...run, agent: "Analyst" } }),
      card(3, { run }),
      card(4),
    ]);
    expect(agentsOf(b)).toEqual(["Analyst", "Coder"]);
  });
});

describe("inspectedCard", () => {
  const b = board([card(1, { column: "Done" }), card(2), card(3, { run }), card(4)]);

  it("is the first live card without a choice", () => {
    expect(inspectedCard(b, [], null)?.id).toBe(3);
  });

  it("is the first card that waits for a person when none is live", () => {
    const quiet = board([card(1, { column: "Done" }), card(2), card(4)]);
    expect(inspectedCard(quiet, [item("approve_issue", 4)], null)?.id).toBe(4);
    expect(inspectedCard(quiet, [], null)?.id).toBe(2);
  });

  it("obeys a choice, and none when the dock was closed", () => {
    expect(inspectedCard(b, [], "4")?.id).toBe(4);
    expect(inspectedCard(b, [], "none")).toBeUndefined();
    expect(inspectedCard(b, [], "99")?.id).toBe(3);
  });
});

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
});

describe("the words and links of a card", () => {
  it("writes tokens as the desktop board does", () => {
    expect(tokens(999)).toBe("999 tok");
    expect(tokens(96_400)).toBe("96k tok");
    expect(tokens(1_250_000)).toBe("1.3M tok");
  });

  it("says the agent and round of a card in coding, and the cost of one that waits to merge", () => {
    expect(cardDetail(card(1, { column: "In progress", run, rounds: 2 }), 5)).toBe(
      "Coder · round 2 of 5",
    );
    expect(cardDetail(card(1, { column: "In progress", rounds: 1 }), 0)).toBe("round 1");
    expect(cardDetail(card(1, { column: "Ready to merge", rounds: 1, tokens: 96_000 }), 5)).toBe(
      "1 round · 96k tok",
    );
    expect(cardDetail(card(1), 5)).toBeNull();
  });

  it("links to GitHub only", () => {
    const c = card(7);
    expect(issueUrl(board([c]).repository, c)).toBe("https://github.com/acme/demo/issues/7");
    expect(pullUrl(board([c]).repository, 12)).toBe("https://github.com/acme/demo/pull/12");
    const none = { ...board([c]).repository, forge: "none" as const, github: null };
    expect(issueUrl(none, c)).toBeNull();
    expect(pullUrl({ ...none, forge: "gitlab", project: "g/p" }, 3)).toBeNull();
  });

  it("badges what runs, blocks and waits, in the order of the desktop board", () => {
    const c = card(1, {
      column: "In progress",
      tokens: 4000,
      blocked_by: [2, 3, 4],
      refusals: 1,
      change: { id: 1, state: "open", checks: "failure", review_verdict: null },
    });
    // The word and the state that colours it, in the order the desktop board draws them; the tones are the shared
    // pill's (`ui/page/Status`), where a fact that is not a state is `neutral`.
    expect(cardBadges(c, "needs-you", true)).toEqual([
      { name: "4k tok", tone: "neutral" },
      { name: "Blocked by #2 and 2 more", tone: "error" },
      { name: "1 rule broken", tone: "error" },
      { name: "CI failed", tone: "error" },
      { name: "Your turn", tone: "attention" },
      { name: "Next", tone: "neutral" },
    ]);
    expect(cardBadges(card(1, { run }), "live", false)).toEqual([
      { name: "Live", tone: "progress" },
    ]);
  });

  it("starts with the first card of Not started that nothing blocks", () => {
    expect(nextCard([card(1, { blocked_by: [9] }), card(2), card(3)])).toBe(2);
    expect(nextCard([card(1, { blocked_by: [9] })])).toBeUndefined();
  });

  it("finds the change of a card: its open one, else the latest", () => {
    const change = (id: number, card_id: number | null, state: Change["state"]): Change => ({
      id,
      repository_id: 1,
      harness_repository: false,
      card_id,
      branch: "b",
      state,
      pull_request: id,
      checks: null,
      review: null,
      head: null,
      published_at: null,
    });
    const b = board([card(1), card(2)], {
      changes: [change(1, 1, "closed"), change(2, 1, "merged"), change(3, 2, "open")],
    });
    expect(changeOf(b, card(1))?.id).toBe(2);
    expect(
      changeOf(
        b,
        card(2, { change: { id: 3, state: "open", checks: "none", review_verdict: null } }),
      )?.id,
    ).toBe(3);
    expect(changeOf(b, card(5))).toBeUndefined();
  });

  it("says what a card waits for", () => {
    expect(waitingLine("held by the repository", 0)).toBe("held by the repository");
    expect(waitingLine(undefined, 1)).toBe("1 rule broken");
    expect(waitingLine("waiting for #99", 2)).toBe("waiting for #99 · 2 rules broken");
    expect(waitingLine(undefined, 0)).toBe("Nothing");
    expect(waitingLine("", 0)).toBe("Nothing");
  });

  it("describes each kind of history entry", () => {
    const d = (kind: string, data: Record<string, unknown>) =>
      describeHistory({ at: 0, kind, data });
    expect(d("moved", { from: "Not started", to: "In progress", note: null })).toBe(
      "Moved from Not started to In progress",
    );
    expect(d("moved", { from: "A", to: "B", note: "why" })).toBe("Moved from A to B: why");
    expect(d("run_started", { run: 3, agent: "Coder", purpose: "code" })).toBe(
      "Run 3 started: Coder, code",
    );
    expect(d("run_ended", { run: 3, status: "failed" })).toBe("Run 3 failed");
    expect(d("decided", { decider: "person", choice: "merge", reason: "ok" })).toBe(
      "person decided merge: ok",
    );
    expect(d("attempts_settled", { runs: [1], picked: null, reason: null })).toBe(
      "No attempt passed",
    );
    expect(d("inbox_added", { item: 1, kind: "stuck", title: "Stuck" })).toBe("Asked you: Stuck");
    expect(d("something_new", {})).toBe("something new");
  });
});
