import { describe, expect, it } from "vitest";
import type { Board, Card, Change, InboxItem, Snapshot } from "../../api/types/board";
import type { Proposal } from "../../api/types/improve";
import {
  approvable,
  changeOf,
  chatOf,
  confirmable,
  draftsOf,
  facts,
  found,
  issuesSent,
  kindTag,
  listOf,
  meta,
  NO_FILTERS,
  proposedOf,
  removeIssue,
  rowReason,
  shown,
  titleOf,
  trialsOf,
  why,
} from "./model";

const card = (id: number, number: number, extra: Partial<Card> = {}): Card => ({
  id,
  repository_id: 1,
  project_id: 1,
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
  parent: null,
  counts_as_sub_issue: true,
  closed: false,
  labels: [],
  claimed_by: null,
  assignees: [],
  priority: "none",
  due: null,
  estimate: null,
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
    home_project_id: 1,
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
): InboxItem => ({
  id,
  kind,
  card_id,
  title: `Item ${id}`,
  data,
  resolved_at: null,
  at,
  assignees: [],
});

const proposed = (extra: Partial<Proposal> = {}): Proposal => ({
  id: 4,
  retro_date: "2026-10-25",
  branch: "proposal/2026-10-25-read-tests",
  title: "Let the Coder read the tests first",
  status: "positive",
  ...extra,
});

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
  it("tags the kinds, the blocked icon for stuck and the merge icon for a change to merge", () => {
    expect(kindTag(item(1, "stuck", 1, {}))).toEqual({ label: "Stuck", mark: "blocked" });
    expect(kindTag(item(1, "ready_to_merge", null, {})).label).toBe("Change from a chat");
    expect(kindTag(item(1, "ready_to_merge", 4, {}))).toEqual({
      label: "Ready to merge",
      mark: "readyToMerge",
    });
    expect(kindTag(item(1, "needs_design", 4, {}))).toEqual({
      label: "Needs design",
      mark: "none",
    });
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
    expect(
      why(item(1, "wiki_proposal", null, { page: "Team/On call", by: "Coder (agent)" }), {}),
    ).toBe(
      "Coder (agent) proposes this edit of the page Team/On call. Merging changes the wiki; dismissing leaves it as it is.",
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
    // A proposal is already the Retro's word, so it carries no source of its own.
    expect(meta(item(1, "proposal", null, {}), {})).toBe("");
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
      { card: 4, outcome: "Positive", tone: "complete", reason: "faster" },
      { card: null, outcome: "No change", tone: "quiet", reason: "" },
    ]);
    expect(trialsOf(proposal, "regression")[0]?.tone).toBe("error");
    expect(trialsOf(item(1, "proposal", null, {}), "trials")).toEqual([]);
  });

  it("shows what a proposal offers, from the item where it can and from the proposal where it cannot", () => {
    // The item made when the Retro stores a proposal carries the change itself and wins over the record.
    const stored = item(1, "proposal", null, {
      kind: "settings",
      reason: "Cards ran out of rounds.",
      diff: "+round_limit = 7",
    });
    expect(proposedOf(stored, proposed({ body: "Other", diff: "+other" }))).toEqual({
      reason: "Cards ran out of rounds.",
      diff: "+round_limit = 7",
    });
    // The item a trial ends with names its proposal and nothing else, so the record stands in for both.
    const fromTrial = item(2, "proposal", null, { proposal: 4 });
    expect(
      proposedOf(fromTrial, proposed({ body: "Read the tests first.", diff: "+read tests" })),
    ).toEqual({
      reason: "Read the tests first.",
      diff: "+read tests",
    });
    // A harness proposal has no diff, and nothing at all to show without its proposal.
    expect(proposedOf(fromTrial, null)).toEqual({ reason: "", diff: "" });
    expect(proposedOf(fromTrial, proposed({ diff: "" })).diff).toBe("");
    // The sentence for such an item comes from the kind the record knows, not from the bare item.
    expect(why(fromTrial, {})).toContain("harness configuration");
    expect(why(fromTrial, {}, "settings")).toContain("moebius.toml");
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

  it("narrows to the items about one change, where the package sheet's link lands", () => {
    // The items 2 and 3 are the ready_to_merge ones; each names its change.
    const only = shown(snapshot, { kind: null, repository: null, change: 2 });
    expect(only.length).toBe(1);
    expect(changeOf(only[0] as InboxItem)).toBe(2);
    expect(shown(snapshot, { kind: null, repository: null, change: 999 })).toEqual([]);
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

describe("a wiki proposal", () => {
  it("is a Wiki edit", () => {
    expect(kindTag(item(1, "wiki_proposal", null, {}))).toEqual({
      label: "Wiki edit",
      mark: "none",
    });
  });
});

describe("a card over several repositories in the Inbox", () => {
  const two: Snapshot = {
    boards: [
      board(1, "demo", [card(45, 45, { repositories: ["demo", "web"] })], [change(2)]),
      board(2, "web", [], [change(3, { repository_id: 2, pull_request: 7 })]),
    ],
    harness_changes: [],
    inbox: [],
  };
  const offer = item(1, "ready_to_merge", 45, { change: 3, branch: "moebius/card-WEB-1" });

  it("finds the card on its own board and the change on the board of its repository", () => {
    const about = found(two, offer);
    expect(about.board?.repository.name).toBe("demo");
    expect(about.card?.id).toBe(45);
    expect(about.change?.id).toBe(3);
    expect(about.changeRepository?.name).toBe("web");
  });

  it("takes the repository of a change from the card when no board lists the change", () => {
    const own = {
      id: 2,
      repository: "demo",
      state: "open",
      checks: "success" as const,
      review_verdict: null,
    };
    const web = {
      id: 3,
      repository: "web",
      state: "open",
      checks: "failure" as const,
      review_verdict: null,
    };
    const unlisted: Snapshot = {
      ...two,
      boards: [
        board(
          1,
          "demo",
          [card(45, 45, { repositories: ["demo", "web"], changes: [own, web] })],
          [change(2)],
        ),
        board(2, "web", [], []),
      ],
    };
    const about = found(unlisted, offer);
    expect(about.changeRepository?.name).toBe("web");
    expect(meta(offer, about)).toBe("web · #45");
  });

  it("names the repository the change is in, in the words above the title and in the facts", () => {
    const about = found(two, offer);
    expect(meta(offer, about)).toBe("web · #45");
    expect(facts(offer, about).find((fact) => fact.label === "Repository")?.value).toBe("web");
  });

  it("adds no repository fact for a card over one repository", () => {
    const single = item(2, "ready_to_merge", 45, { change: 2, branch: "moebius/card-45" });
    const one = found(snapshot, single);
    expect(facts(single, one).some((fact) => fact.label === "Repository")).toBe(false);
  });

  it("says what a stopped merge left, as a kind of its own", () => {
    const stopped = item(2, "merge_stopped", 45, {
      change: 3,
      repository: "web",
      reason: "the change no longer merges",
      merged: ["demo"],
      open: ["web", "docs"],
    });
    const about = found(two, stopped);
    expect(kindTag(stopped)).toEqual({ label: "Merge stopped", mark: "blocked" });
    expect(why(stopped, about)).toBe(
      "The merge in web stopped: the change no longer merges. Merged: demo. Still open: web, docs.",
    );
  });
});

describe("the list of the page", () => {
  const demo = (cards: Card[]): Board => board(1, "demo", cards, []);
  const mine = (id: number, extra: Partial<Card> = {}): Card => card(id, id, extra);
  const waiting = (id: number, kind: string, cardId: number | null, at = 10): InboxItem => ({
    id,
    kind,
    card_id: cardId,
    title: `Item ${id}`,
    data: {},
    resolved_at: null,
    at,
    assignees: ["leon"],
  });
  const none = { kind: null, repository: null, change: null };

  it("groups what needs the person, what runs and what is done, each card once", () => {
    const snap = {
      boards: [
        demo([
          mine(1),
          mine(2, {
            run: { id: 9, agent: "coder", purpose: "code", status: "running" },
            rounds: 2,
          }),
          mine(3, { closed: true }),
        ]),
      ],
      harness_changes: [],
      inbox: [waiting(5, "approve_issue", 1)],
    } as unknown as Snapshot;
    const list = listOf(snap, shown(snap, none), none);
    expect(list.map((each) => (each.type === "group" ? each.name : each.key))).toEqual([
      "Needs you",
      "item-5",
      "Running",
      "card-2",
      "Earlier",
      "card-3",
    ]);
    const rows = list.flatMap((each) => (each.type === "row" ? [each.data] : []));
    expect(rows.map((each) => [each.status, each.reason])).toEqual([
      ["needsYou", "Waiting for approval"],
      ["running", "Coding, round 2"],
      ["done", "Done"],
    ]);
    expect(rows[0]?.approves).toBe(1);
    expect(rows[1]?.approves).toBeNull();
    // Only a card in Earlier has an age (the card's own); one at work has none.
    expect(rows.map((each) => each.at)).toEqual([expect.any(Number), null, 0]);
  });

  it("titles a row of a card with the card's title, not the sentence the server worded the item with", () => {
    const snap = {
      boards: [
        { repository: { id: 1 }, cards: [card(1, 1, { title: "Cart saves" })], changes: [] },
      ],
      harness_changes: [],
      inbox: [
        {
          ...waiting(5, "approve_issue", 1),
          title:
            "The review found a data loss: a save built on an older cart replaces the newer one.",
        },
      ],
    } as unknown as Snapshot;
    const row = listOf(snap, shown(snap, none), none).find((each) => each.type === "row");
    expect(row?.type === "row" && [row.data.title, row.data.reason]).toEqual([
      "Cart saves",
      "Waiting for approval",
    ]);
  });

  it("says the reason in the pale text only, not in the title", () => {
    const item = (title: string) => ({ ...waiting(1, "ready_to_merge", 4), title });
    expect(titleOf(item("Add an index is ready to merge"))).toBe("Add an index");
    expect(titleOf({ ...item("Approve: Support Apple Pay"), kind: "approve_issue" })).toBe(
      "Support Apple Pay",
    );
    expect(titleOf(item("Tell the Coder to run the tests first"))).toBe(
      "Tell the Coder to run the tests first",
    );
  });

  it("draws only the items under a change filter, and no group for nothing", () => {
    const snap = {
      boards: [demo([mine(2, { closed: true })])],
      harness_changes: [],
      inbox: [],
    } as unknown as Snapshot;
    expect(listOf(snap, [], { ...none, change: 3 })).toEqual([]);
    expect(listOf(snap, [], none).map((each) => each.key)).toEqual(["group-earlier", "card-2"]);
  });

  it("offers Approve only for an issue that waits for approval and has a card", () => {
    expect(approvable(waiting(1, "approve_issue", 4))).toBe(true);
    expect(approvable(waiting(1, "approve_issue", null))).toBe(false);
    expect(approvable(waiting(1, "stuck", 4))).toBe(false);
    // No kind says "Plan ready": nothing on the server proposes a plan and waits for it.
    expect(rowReason(waiting(1, "approve_issue", 4))).toBe("Waiting for approval");
    expect(rowReason(waiting(1, "ready_to_merge", 4))).toBe("Ready to merge");
    expect(rowReason(waiting(1, "change_closed", 4))).toBe("Closed without a merge");
    expect(rowReason(waiting(1, "stuck", 4))).toBe("Stuck");
    expect(rowReason({ ...waiting(1, "stuck", 4), data: { reason: "no progress" } })).toBe(
      "Stuck: no progress",
    );
  });
});

describe("the tag of a done question", () => {
  it("is a word of its own", async () => {
    const { kindTag } = await import("./model");
    const item = {
      id: 1,
      kind: "done_asked",
      card_id: 5,
      title: "t",
      data: {},
      resolved_at: null,
      at: 1,
      assignees: [],
    };
    expect(kindTag(item)).toEqual({ label: "Says it is done", mark: "none" });
  });
});
