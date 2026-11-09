import { describe, expect, it } from "vitest";
import type { Board, Card, Change, InboxItem } from "../../api/types/board";
import type { Run } from "../../api/types/cards";
import {
  agentsOf,
  boardColumns,
  cardBadges,
  cardDetail,
  cardStatus,
  changeOf,
  changeWorktree,
  DONE_SHOWN,
  descendants,
  describeHistory,
  dropRule,
  groupByColumn,
  heldLine,
  inboxOf,
  inspectedCard,
  isClosed,
  issueUrl,
  labelsOf,
  NO_FILTERS,
  nextCard,
  parentOf,
  pullRequestOf,
  pullUrl,
  rootOf,
  type Stack,
  stackWords,
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
    parent: null,
    counts_as_sub_issue: true,
    closed: false,
    labels: [],
    claimed_by: null,
    ...extra,
  };
}

/** A card held by a person with a reason. */
const held = { reason: "wait for the design", by: "person", at: 1 };

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

const run = running;

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
    // A card that is gone — deleted, in this tab or in another — leaves the dock closed rather than opening another.
    expect(inspectedCard(b, [], "99")).toBeUndefined();
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

describe("the words and links of a card", () => {
  it("writes tokens as the desktop board does", () => {
    expect(tokens(999)).toBe("999 tok");
    expect(tokens(96_400)).toBe("96k tok");
    expect(tokens(1_250_000)).toBe("1.3M tok");
  });

  it("says the agent and round of a card in coding, and the rounds of one that waits to merge", () => {
    expect(cardDetail(card(1, { column: "In progress", run, rounds: 2 }), 5)).toBe(
      "Coder · round 2 of 5",
    );
    expect(cardDetail(card(1, { column: "In progress", rounds: 1 }), 0)).toBe("round 1");
    // What a card cost is the dock's row, not a line on the card.
    expect(cardDetail(card(1, { column: "Ready to merge", rounds: 1, tokens: 96_000 }), 5)).toBe(
      "1 round",
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

  it("names the pull request of a change and where to open it", () => {
    const repository = board([card(1)]).repository;
    const change = {
      id: 4,
      repository_id: 1,
      harness_repository: false,
      card_id: 1,
      branch: "b",
      state: "open",
      pull_request: 12,
      checks: null,
      review: null,
      head: null,
      published_at: null,
    } satisfies Change;
    expect(pullRequestOf(repository, change)).toEqual({
      number: 12,
      url: "https://github.com/acme/demo/pull/12",
      status: "open",
    });
    // A change not yet published has no pull request to open.
    expect(pullRequestOf(repository, { ...change, pull_request: null })).toBeNull();
    expect(pullRequestOf(repository, undefined)).toBeNull();
    const none = { ...repository, forge: "none" as const, github: null };
    expect(pullRequestOf(none, change)?.url).toBeNull();
  });

  it("badges what runs, blocks and waits, in the order of the desktop board", () => {
    const c = card(1, {
      column: "In progress",
      blocked_by: [2, 3, 4],
      refusals: 1,
      change: { id: 1, state: "open", checks: "failure", review_verdict: null },
    });
    // The word and the state that colours it, in the order the desktop board draws them; the tones are the shared
    // pill's (`ui/page/Status`), where a fact that is not a state is `neutral`.
    expect(cardBadges(c, "needs-you", true)).toEqual([
      { name: "Waiting on #2 and 2 more", tone: "attention" },
      { name: "1 rule broken", tone: "error" },
      { name: "CI failed", tone: "error" },
      { name: "Your turn", tone: "attention" },
      { name: "Next", tone: "neutral" },
    ]);
    expect(cardBadges(card(1, { run }), "live", false)).toEqual([
      { name: "Live", tone: "progress" },
    ]);
  });

  it("starts with the first card of Not started that nothing blocks and nobody holds", () => {
    expect(nextCard([card(1, { blocked_by: [9] }), card(2), card(3)])).toBe(2);
    expect(nextCard([card(1, { blocked_by: [9] })])).toBeUndefined();
    expect(nextCard([card(1, { held }), card(2), card(3)])).toBe(2);
    expect(nextCard([card(1, { held })])).toBeUndefined();
  });

  it("says a held card is Blocked first, apart from the card that waits on issues", () => {
    const c = card(1, { column: "Not started", held, blocked_by: [4] });
    expect(cardBadges(c, "held", false)).toEqual([
      { name: "Blocked", tone: "error", hint: "wait for the design (blocked by a person)" },
      { name: "Waiting on #4", tone: "attention" },
    ]);
    expect(cardBadges(card(2, { blocked_by: [4] }), "blocked", false)).toEqual([
      { name: "Waiting on #4", tone: "attention" },
    ]);
  });

  it("tells who held a card and why", () => {
    expect(heldLine(held)).toBe("wait for the design (blocked by a person)");
    expect(heldLine({ reason: null, by: "agent:Moebius", at: 1 })).toBe("Blocked by Moebius");
    expect(heldLine({ reason: "  ", by: "person", at: 1 })).toBe("Blocked by a person");
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

describe("the worktree a card's change was made in", () => {
  function run(id: number, purpose: string, status: Run["status"], worktree: string | null): Run {
    return {
      id,
      card_id: 1,
      agent: "coder",
      harness: "pi-bolt",
      purpose,
      status,
      worktree,
      started_at: 0,
      ended_at: null,
      summary: null,
      repository_id: 1,
      harness_repository: false,
      origin: "card",
      runner: null,
      title: null,
    };
  }

  it("is the code run's, not a later review run's", () => {
    const runs = [run(1, "code", "waiting", "/wt/code"), run(2, "review", "running", "/wt/review")];
    expect(changeWorktree(card(1), runs)).toBe("/wt/code");
  });

  it("is the picked attempt's", () => {
    const attempts = [
      {
        run: 3,
        model: null,
        status: "waiting",
        result: "failed",
        output: "",
        picked: false,
        reason: null,
      },
      {
        run: 4,
        model: null,
        status: "waiting",
        result: "passed",
        output: "",
        picked: true,
        reason: null,
      },
    ] as Card["attempts"];
    const runs = [run(3, "code", "waiting", "/wt/a"), run(4, "code", "waiting", "/wt/b")];
    expect(changeWorktree(card(1, { attempts }), runs)).toBe("/wt/b");
  });

  it("is none without a worktree or once the run is over", () => {
    expect(changeWorktree(card(1), [run(1, "code", "running", null)])).toBeNull();
    expect(changeWorktree(card(1), [run(1, "code", "succeeded", "/wt/gone")])).toBeNull();
    expect(changeWorktree(card(1), [])).toBeNull();
  });
});
