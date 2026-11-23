/**
 * Tests of the words and links of a card: its details, badges, history and worktree.
 */
import { describe, expect, it } from "vitest";
import type { Card, Change } from "../../../api/types/board";
import type { Run } from "../../../api/types/cards";
import { board, card, held, run } from "./fixtures";
import { waitingWords } from "./inbox";
import {
  cardActivity,
  cardBadges,
  cardDetail,
  changeOf,
  changeWorktree,
  describeHistory,
  elsewhereBy,
  heldLine,
  issueUrl,
  nextCard,
  pullRequestOf,
  pullUrl,
  tokens,
  waitingLine,
} from "./index";

describe("what a running card says", () => {
  const running = (purpose: string, rounds: number) =>
    card(1, { run: { id: 1, agent: "Coder", purpose, status: "running" }, rounds });
  it("says in a few words what the agent is doing, with its round", () => {
    expect(cardActivity(running("code", 1), "live")).toMatchObject({ text: "coding, round 1" });
    expect(cardActivity(running("review", 2), "live")).toMatchObject({
      text: "reviewing, round 2",
    });
    expect(cardActivity(running("plan", 0), "live")).toMatchObject({ text: "planning" });
    expect(cardActivity(running("code", 1), "live")?.hint).toBe("Coder");
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
    // The word and the mark before it, in the order the desktop board draws them; only a blocked card, a stuck one and
    // a live run have a mark (`ui/page/MarkLabel`), the rest is grey text.
    expect(cardBadges(c, "needs-you", true)).toEqual([
      { name: "Waiting on #2 and 2 more", mark: "none" },
      { name: "1 rule broken", mark: "none" },
      { name: "CI failed", mark: "none" },
      { name: "Your turn", mark: "none" },
      { name: "Next", mark: "none" },
    ]);
    expect(cardBadges(card(1, { run }), "live", false)).toEqual([
      { name: "Live", mark: "live", detail: run.agent },
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
      { name: "Blocked", mark: "blocked", hint: "wait for the design (blocked by a person)" },
      { name: "Waiting on #4", mark: "none" },
    ]);
    expect(cardBadges(card(2, { blocked_by: [4] }), "blocked", false)).toEqual([
      { name: "Waiting on #4", mark: "none" },
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

describe("what a card that needs a person waits for", () => {
  it("says it by the kind of the open item, and never as a plan", async () => {
    const { waitingWords } = await import("./inbox");
    expect(waitingWords("approve_issue")).toBe("Waiting for approval");
    expect(waitingWords("ready_to_merge")).toBe("Ready to merge");
    expect(waitingWords("change_closed")).toBe("Closed without a merge");
    expect(waitingWords(undefined)).toBe("Your turn");
  });
});

describe("the second line of a card that needs a person", () => {
  it("uses the words it is given, and 'Your turn' when it is given none", () => {
    expect(cardActivity(card(1), "needs-you", "Waiting for approval")?.text).toBe(
      "Waiting for approval",
    );
    expect(cardActivity(card(1), "needs-you")?.text).toBe("Your turn");
  });
});

describe("the line of a card that is someone else's work", () => {
  const change = { id: 3, state: "open", checks: "success", review_verdict: null } as const;

  it("says who took an issue on another server", () => {
    const taken = card(1, { taken_by: "alice" });
    expect(cardActivity(taken, "idle")).toMatchObject({ text: "Taken by @alice", tone: "quiet" });
    expect(cardActivity(taken, "waiting")).toMatchObject({ text: "Taken by @alice" });
    // What a person must do outranks it, and a card of this server's own has no such line.
    expect(cardActivity(taken, "needs-you", "Your turn")).toMatchObject({ text: "Your turn" });
    expect(cardActivity(card(1), "idle")).toBeNull();
  });

  it("says an issue was taken with /take, and that a card whose assignee says it is done waits for the owner", () => {
    const taken = card(1, { taken_by: "alice", taken_by_command: true });
    expect(cardActivity(taken, "idle")).toMatchObject({
      text: "Taken by @alice with /take",
      tone: "quiet",
    });
    expect(waitingWords("done_asked")).toBe("Says it is done");
    expect(waitingWords("review_rounds_spent")).toBe("Reviewed three times");
    expect(waitingWords("review_not_possible")).toBe("Could not be reviewed");
    expect(cardActivity(taken, "needs-you", waitingWords("done_asked"))).toMatchObject({
      text: "Says it is done",
      tone: "needs-you",
    });
  });

  it("asks the owner to review a pull request someone else opened, and says whose it is after", () => {
    const followed = card(1, { taken_by: "alice", change: { ...change, outside_author: "alice" } });
    expect(cardActivity(followed, "needs-you", "Pull request to review")).toMatchObject({
      text: "Review @alice's pull request",
      tone: "needs-you",
    });
    expect(cardActivity(followed, "idle")).toMatchObject({
      text: "Pull request by @alice",
      tone: "quiet",
    });
    expect(elsewhereBy(followed)).toBe("alice");
    expect(elsewhereBy(card(1, { taken_by: "bo" }))).toBe("bo");
    expect(elsewhereBy(card(1))).toBeNull();
  });
});
