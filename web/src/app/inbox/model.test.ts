import { describe, expect, it } from "vitest";
import type { Board, Card, Change, InboxItem, Snapshot } from "../../api/types/board";
import {
  chatOf,
  chipName,
  confirmable,
  count,
  draftsOf,
  facts,
  found,
  issuesSent,
  kindsOf,
  kindTag,
  meta,
  NO_FILTERS,
  removeIssue,
  shown,
  summary,
  trialsOf,
  why,
} from "./model";

const card = (id: number, number: number, extra: Partial<Card> = {}): Card => ({
  id,
  repository_id: 1,
  number,
  title: `Card ${number}`,
  body: "",
  author: "person",
  column: "Ready to merge",
  created_at: 0,
  note: null,
  retried_after: null,
  rounds: 2,
  tokens: 590_000,
  run: null,
  blocked_by: [],
  refusals: 0,
  change: null,
  ...extra,
});

const change = (id: number, extra: Partial<Change> = {}): Change => ({
  id,
  repository_id: 1,
  harness_repository: false,
  card_id: 45,
  branch: "moebius/card-45",
  state: "open",
  pull_request: 18,
  checks: "success",
  review: "approved",
  head: null,
  published_at: null,
  ...extra,
});

const board = (id: number, name: string, cards: Card[], changes: Change[]): Board => ({
  repository: {
    id,
    name,
    path: "/r",
    forge: "none",
    default_branch: "main",
    github: null,
    trusted: [],
  },
  cards,
  round_limit: 5,
  waiting: {},
  changes,
});

const item = (
  id: number,
  kind: string,
  card_id: number | null,
  data: Record<string, unknown>,
  at = 100,
): InboxItem => ({ id, kind, card_id, title: `Item ${id}`, data, resolved_at: null, at });

const snapshot: Snapshot = {
  boards: [
    board(
      1,
      "radiance",
      [card(45, 45), card(55, 55, { column: "Backlog", author: "lowfreq" })],
      [change(2)],
    ),
    board(2, "moebius", [card(12, 12, { column: "In progress" })], []),
  ],
  harness_changes: [change(9, { harness_repository: true, branch: "moebius/harness" })],
  inbox: [
    item(1, "stuck", 12, { reason: "the round limit is reached" }, 300),
    item(2, "ready_to_merge", 45, { change: 2, branch: "moebius/card-45" }, 200),
    item(3, "ready_to_merge", null, { change: 9, branch: "moebius/chat-9" }, 100),
    item(4, "approve_issue", 55, { reason: "lowfreq is not a trusted author" }, 400),
    item(5, "needs_design", 12, {}, 50),
  ],
};

describe("what an item is about", () => {
  it("finds the board, the card and the change of an item, the harness repository's change too", () => {
    const [stuck, merge, chat] = snapshot.inbox as [InboxItem, InboxItem, InboxItem];
    expect(found(snapshot, stuck).board?.repository.name).toBe("moebius");
    expect(found(snapshot, merge).card?.number).toBe(45);
    expect(found(snapshot, merge).change?.id).toBe(2);
    expect(found(snapshot, chat).change?.harness_repository).toBe(true);
    expect(found(snapshot, chat).board).toBeUndefined();
  });

  it("finds the board of a grilling or a proposal by its repository", () => {
    const grilling = item(7, "grilling_issues", null, { grilling: 3, repository: 2, issues: [] });
    expect(found(snapshot, grilling).board?.repository.name).toBe("moebius");
    const other = item(8, "takeover", null, { takeover: 1, repository: 2 });
    expect(found(snapshot, other).board).toBeUndefined();
  });

  it("reads the chat of a branch", () => {
    expect(chatOf(item(1, "ready_to_merge", null, { branch: "moebius/chat-9" }))).toBe(9);
    expect(chatOf(item(1, "ready_to_merge", 4, { branch: "moebius/card-4" }))).toBeNull();
    expect(chatOf(item(1, "ready_to_merge", null, { branch: "moebius/chat-x" }))).toBeNull();
  });
});

describe("how an item tells itself", () => {
  it("tags the kinds, red for stuck and amber for what waits for a move", () => {
    expect(kindTag(item(1, "stuck", 1, {}))).toEqual({ label: "Stuck", tone: "error" });
    expect(kindTag(item(1, "ready_to_merge", null, {})).label).toBe("Change from a chat");
    expect(kindTag(item(1, "ready_to_merge", 4, {})).label).toBe("Ready to merge");
    expect(kindTag(item(1, "needs_design", 4, {}))).toEqual({
      label: "Needs design",
      tone: "neutral",
    });
    expect(chipName("approve_issue")).toBe("Approvals");
    expect(chipName("needs_design")).toBe("Needs design");
  });

  it("says why an item is there", () => {
    const [stuck, , , approve, script] = snapshot.inbox as InboxItem[];
    const about = (each: InboxItem) => found(snapshot, each);
    expect(why(stuck as InboxItem, about(stuck as InboxItem))).toBe(
      "Stopped: the round limit is reached.",
    );
    expect(why(approve as InboxItem, about(approve as InboxItem))).toBe(
      "A person has to approve this issue: lowfreq is not a trusted author.",
    );
    expect(why(item(1, "stuck", 1, {}), {})).toBe("The card is stuck.");
    expect(why(item(1, "ready_to_merge", 1, {}), {})).toBeNull();
    expect(why(item(1, "change_closed", 1, { pull_request: 18 }), {})).toBe(
      "Pull request #18 was closed without a merge.",
    );
    expect(why(item(1, "change_closed", 1, {}), { change: change(2, { pull_request: 7 }) })).toBe(
      "Pull request #7 was closed without a merge.",
    );
    expect(why(item(1, "takeover", null, { agreement: 0.95, count: 120 }), {})).toContain(
      "95% of 120 judgements",
    );
    expect(why(item(1, "proposal", null, { kind: "settings" }), {})).toContain("moebius.toml");
    expect(why(item(1, "proposal", null, {}), {})).toContain("harness configuration");
    expect(why(script as InboxItem, {})).toBeNull();
    expect(why(item(1, "needs_design", 1, { reason: "no mockup" }), {})).toBe("no mockup");
  });

  it("names the repository, the card, the author of an outside issue and the source of the rest", () => {
    const [stuck, merge, chat, approve] = snapshot.inbox as InboxItem[];
    const about = (each: InboxItem) => found(snapshot, each);
    expect(meta(stuck as InboxItem, about(stuck as InboxItem))).toBe("moebius · #12");
    expect(meta(merge as InboxItem, about(merge as InboxItem))).toBe("radiance · #45");
    expect(meta(chat as InboxItem, about(chat as InboxItem))).toBe("Harness configuration · chat");
    expect(meta(approve as InboxItem, about(approve as InboxItem))).toBe(
      "radiance · #55 · by lowfreq",
    );
    expect(meta(item(1, "proposal", null, {}), {})).toBe("Retro");
    expect(meta(item(1, "grilling_report", null, { grilling: 1 }), {})).toBe("Grilling");
  });

  it("lists the facts of a change that waits", () => {
    const merge = snapshot.inbox[1] as InboxItem;
    expect(facts(merge, found(snapshot, merge))).toEqual([
      { label: "Rounds", value: "2" },
      { label: "Tokens", value: "590k tok" },
      { label: "Checks", value: "CI green · review approved", good: true },
      { label: "Branch", value: "moebius/card-45", mono: true },
    ]);
    const chat = snapshot.inbox[2] as InboxItem;
    expect(facts(chat, found(snapshot, chat)).map((fact) => fact.label)).toEqual([
      "Checks",
      "Branch",
    ]);
    expect(
      facts(item(1, "ready_to_merge", 1, {}), {
        change: change(1, { checks: "pending", review: null }),
      })[0],
    ).toEqual({
      label: "Checks",
      value: "CI running",
      good: false,
    });
    expect(facts(item(1, "ready_to_merge", 1, {}), {})[0]?.value).toBe("No checks");
  });

  it("lists the trials of a proposal and its regression set", () => {
    const proposal = item(1, "proposal", null, {
      trials: [
        { card: 4, outcome: "positive", reason: "faster" },
        { card: null, outcome: "no_change" },
      ],
      regression: [{ card: 2, outcome: "negative", reason: "broke it" }],
    });
    expect(trialsOf(proposal, "trials")).toEqual([
      { card: "Card 4", outcome: "Positive", positive: true, reason: "faster" },
      { card: "No card", outcome: "No change", positive: false, reason: "" },
    ]);
    expect(trialsOf(proposal, "regression")[0]?.positive).toBe(false);
    expect(trialsOf(item(1, "proposal", null, {}), "trials")).toEqual([]);
  });
});

describe("the filters", () => {
  it("shows the newest first, and by kind and repository", () => {
    expect(shown(snapshot, NO_FILTERS).map((each) => each.id)).toEqual([4, 1, 2, 3, 5]);
    expect(
      shown(snapshot, { kind: "ready_to_merge", repository: null }).map((each) => each.id),
    ).toEqual([2, 3]);
    expect(shown(snapshot, { kind: null, repository: 2 }).map((each) => each.id)).toEqual([1, 5]);
    expect(shown(snapshot, { kind: "stuck", repository: 1 })).toEqual([]);
  });

  it("counts a chip against the other filter", () => {
    expect(count(snapshot, null, null)).toBe(5);
    expect(count(snapshot, "ready_to_merge", null)).toBe(2);
    expect(count(snapshot, "ready_to_merge", 1)).toBe(1);
    expect(count(snapshot, null, 2)).toBe(2);
  });

  it("lists the kinds present, the core's in order and then the scripts' by name", () => {
    expect(kindsOf(snapshot)).toEqual(["ready_to_merge", "approve_issue", "stuck", "needs_design"]);
  });

  it("tells how many things need the person", () => {
    // An empty inbox is said by the empty state of the page, and not twice over.
    expect(summary(0, 0)).toBe("");
    expect(summary(1, 1)).toBe("1 thing needs you.");
    expect(summary(5, 5)).toBe("5 things need you.");
    expect(summary(5, 2)).toBe("5 things need you; 2 shown.");
  });
});

describe("the issues of a grilling", () => {
  const grilling = item(3, "grilling_issues", null, {
    grilling: 7,
    repository: 1,
    issues: [
      { title: "A", body: "First.", blocked_by: [] },
      { title: "B", body: "Second.", blocked_by: [0] },
      { title: "C", body: "Third.", blocked_by: [0, 1] },
    ],
  });

  it("removes an issue, drops it from the blockers and moves the indexes after it up", () => {
    const drafts = removeIssue(draftsOf(grilling), 0);
    expect(drafts.map((draft) => [draft.title, draft.blockedBy])).toEqual([
      ["B", []],
      ["C", [0]],
    ]);
    expect(removeIssue(drafts, 5)).toBe(drafts);
  });

  it("sends the list trimmed and cannot confirm a list without an issue or with a blank title", () => {
    const drafts = draftsOf(grilling);
    expect(confirmable(drafts)).toBe(true);
    const edited = drafts.map((draft, at) =>
      at === 1 ? { ...draft, title: "  ", body: " x " } : draft,
    );
    expect(confirmable(edited)).toBe(false);
    expect(issuesSent(edited)[1]).toEqual({ title: "", body: "x", blocked_by: [0] });
    expect(confirmable([])).toBe(false);
  });
});
