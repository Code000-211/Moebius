import { describe, expect, it } from "vitest";
import type { Card, CardRun, Snapshot } from "../../api/types/board";
import type { Chat } from "../../api/types/chat";
import type { FinishedRun } from "../../api/types/runs";
import { type Entry, type ToolCall, toolInput } from "../../chat/conversation";
import {
  afterSteps,
  doingWords,
  elapsed,
  finishedId,
  finishedItems,
  finishedKey,
  items,
  lasted,
  latestSteps,
  tabOf,
  thinkingNow,
  thoughtSeconds,
  tokensText,
  type Work,
  waitingWord,
  working,
  workStartedAt,
} from "./model";

const card = (id: number, number: number, run: CardRun | null, tokens = 0): Card => ({
  id,
  repository_id: 1,
  project_id: 1,
  number,
  title: `Card ${number}`,
  body: "",
  author: "person",
  column: "In progress",
  created_at: 0,
  note: null,
  retried_after: null,
  rounds: 0,
  tokens,
  run,
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
});

const run = (id: number, status: CardRun["status"]): CardRun => ({
  id,
  agent: "Coder",
  purpose: "code",
  status,
});

const chat = (id: number, extra: Partial<Chat>): Chat => ({
  id,
  agent: "Moebius",
  status: "running",
  repository_id: 1,
  harness_repository: false,
  started_at: 1000,
  ended_at: null,
  title: "A chat",
  answered: 0,
  seen: 0,
  changed_files: 0,
  asks: false,
  origin: "chat",
  model: null,
  thinking: null,
  owner: null,
  shared: false,
  mine: true,
  ...extra,
});

const snapshot: Snapshot = {
  boards: [
    {
      repository: {
        id: 1,
        name: "radiance",
        path: "/r",
        forge: "none",
        default_branch: "main",
        home_project_id: 1,
        github: null,
        trusted: [],
      },
      cards: [
        card(1, 48, run(9, "running"), 412_000),
        card(2, 47, run(11, "waiting")),
        card(3, 45, run(8, "succeeded")),
        card(4, 44, null),
      ],
      round_limit: 5,
      waiting: {},
      changes: [],
    },
  ],
  harness_changes: [],
  inbox: [],
};

describe("which runs are working", () => {
  it("lists the cards whose run is starting, running or waiting and the chats that are, by run", () => {
    const list = working(snapshot, [
      chat(7, { title: "" }),
      chat(5, { status: "succeeded" }),
      chat(6, { status: "waiting", asks: true, repository_id: null, harness_repository: true }),
    ]);
    expect(list.map((each) => each.run)).toEqual([6, 7, 9, 11]);
    expect(list.find((each) => each.run === 9)).toMatchObject({
      title: "Card 48",
      id: "#48",
      repository: "radiance",
      tokens: 412_000,
      kind: "code",
      open: { kind: "card", repository: 1, card: 1 },
    });
    expect(list.find((each) => each.run === 11)?.tokens).toBeNull();
    expect(list.find((each) => each.run === 7)).toMatchObject({
      title: "Chat",
      kind: "chat",
      open: { kind: "chat", chat: 7 },
    });
    expect(list.find((each) => each.run === 6)).toMatchObject({
      asks: true,
      repository: "harness configuration",
    });
  });

  it("names a card by its issue id and keeps its title apart", () => {
    const list = working(snapshot, []);
    expect(list.find((each) => each.run === 9)).toMatchObject({ id: "#48", title: "Card 48" });
    expect(list.find((each) => each.run === 11)?.approve).toBeNull();
  });

  it("offers the approval of a waiting card that has an open approve item", () => {
    const item = (card_id: number, resolved_at: number | null) => ({
      id: card_id,
      kind: "approve_issue",
      card_id,
      title: "",
      data: {},
      resolved_at,
      at: 1,
      assignees: [],
    });
    const list = working({ ...snapshot, inbox: [item(2, null), item(1, null)] }, []);
    expect(list.find((each) => each.run === 11)?.approve).toBe(2);
    // A run that is going has no plan to approve; an item that was resolved asks for nothing.
    expect(list.find((each) => each.run === 9)?.approve).toBeNull();
    expect(
      working({ ...snapshot, inbox: [item(2, 5)] }, []).find((each) => each.run === 11)?.approve,
    ).toBeNull();
  });
});

describe("the list the page draws", () => {
  const run = (at: number, status: string, asks = false, approve: number | null = null) =>
    ({ run: at, status, asks, approve, open: { kind: "card", card: at } }) as Work;

  it("puts the runs that wait for the person first, with no group rows", () => {
    const list = [run(1, "running"), run(2, "waiting"), run(3, "starting"), run(4, "waiting")];
    expect(items(list).map((item) => item.data.run)).toEqual([2, 4, 1, 3]);
    expect(items([])).toEqual([]);
  });

  it("says why a run waits", () => {
    expect(waitingWord(run(1, "waiting", false, 1))).toBe("Waiting for approval");
    expect(waitingWord(run(1, "waiting", true))).toBe("Asks you");
    expect(waitingWord(run(1, "waiting"))).toBe("Your turn");
  });
});

describe("how a tile tells its run", () => {
  it("tells tokens and time", () => {
    expect(tokensText(412_000)).toBe("412k tokens");
    expect(tokensText(1_500_000)).toBe("1.5M tokens");
    expect(tokensText(12)).toBe("12 tokens");
    expect(elapsed(42)).toBe("42s");
    expect(elapsed(437)).toBe("7m 17s");
    expect(elapsed(3720)).toBe("1h 02m");
    expect(elapsed(-5)).toBe("0s");
  });

  it("says how long a thought went on, and which one is still going", () => {
    const thought = (at: number): Entry => ({ at, seq: at, kind: "thinking", text: "t" });
    const tool = (at: number): Entry =>
      ({ at, seq: at, kind: "tool", call: { at } }) as unknown as Entry;
    const text = (at: number): Entry => ({ at, seq: at, kind: "text", text: "a" });
    // A thought runs until whatever follows it begins.
    expect(thoughtSeconds([thought(100), thought(105), tool(112)], 0)).toBe(5);
    expect(thoughtSeconds([thought(100), thought(105), tool(112)], 1)).toBe(7);
    // The last line is measured against whatever followed the steps at all, and a tool is not a thought.
    expect(thoughtSeconds([thought(100), thought(105)], 1, 112)).toBe(7);
    expect(thoughtSeconds([thought(100), thought(105)], 1)).toBeNull();
    expect(thoughtSeconds([thought(100), tool(105)], 1, 112)).toBeNull();
    // The time whatever came after the last step arrived, and none when the steps are the end of it.
    expect(afterSteps([text(0), thought(100), thought(105), text(109)])).toBe(109);
    expect(afterSteps([text(0), thought(100)])).toBeNull();
  });

  it("measures the work from the start of the run when only its newest messages are held", () => {
    const person = (at: number): Entry => ({
      at,
      seq: at,
      kind: "person",
      said: "said",
      message: "m",
    });
    // Everything is held: the first message is where the work began.
    expect(workStartedAt([person(300), person(400)], false, 100)).toBe(300);
    // A window of a long run begins long after the run did, so its first message would understate the work.
    expect(workStartedAt([person(300), person(400)], true, 100)).toBe(100);
    expect(workStartedAt([person(300)], true, 0)).toBeNull();
    // Nothing read yet says nothing of its own.
    expect(workStartedAt([], false, 100)).toBe(100);
    expect(workStartedAt([], false, 0)).toBeNull();
  });

  it("knows a run that waits for the person is not thinking", () => {
    expect(thinkingNow("starting")).toBe(true);
    expect(thinkingNow("running")).toBe(true);
    expect(thinkingNow("waiting")).toBe(false);
    expect(thinkingNow("done")).toBe(false);
  });

  it("shows the last run of steps", () => {
    const text: Entry = { at: 0, seq: 0, kind: "text", text: "a" };
    const thought: Entry = { at: 0, seq: 1, kind: "thinking", text: "t" };
    expect(latestSteps([text])).toEqual([]);
    expect(latestSteps([thought, text, thought, thought, text])).toHaveLength(2);
    expect(latestSteps([thought, thought])).toHaveLength(2);
    expect(latestSteps([])).toEqual([]);
  });
});

describe("the finished runs", () => {
  const ended = (id: number, status: string, endedAt: number | null, tokens = 0): FinishedRun => ({
    run: {
      id,
      card_id: 1,
      agent: "Coder",
      harness: "pi-bolt",
      purpose: "code",
      status: status as FinishedRun["run"]["status"],
      worktree: null,
      started_at: 1_700_000_000,
      ended_at: endedAt,
      summary: null,
      repository_id: null,
      harness_repository: false,
      origin: "chat",
      runner: null,
      title: null,
    },
    card: {
      id: 1,
      repository_id: 1,
      number: 24,
      project_key: "WEB",
      project_number: 24,
      title: "A card",
    },
    tokens,
  });
  const noon = (days: number) => {
    const at = new Date(2026, 10, 20 - days, 12, 0, 0);
    return Math.floor(at.getTime() / 1000);
  };

  it("keeps the address's tab, and reads anything but finished as Active", () => {
    expect(tabOf("finished")).toBe("finished");
    expect(tabOf("active")).toBe("active");
    expect(tabOf(null)).toBe("active");
    expect(tabOf("nonsense")).toBe("active");
  });

  it("lists only runs that ended, newest first, under the day they ended on", () => {
    const list = finishedItems(
      [
        ended(1, "succeeded", noon(2)),
        ended(2, "failed", noon(0)),
        ended(3, "running", null),
        ended(4, "stopped", noon(0) + 60),
        ended(5, "succeeded", noon(1)),
      ],
      "2026-11-20",
    );
    expect(
      list.map((one) => (one.type === "group" ? one.name : (one.data as FinishedRun).run.id)),
    ).toEqual(["Today", 4, 2, "Yesterday", 5, "18 Nov", 1]);
  });

  it("names a card by its key and says how long a run went on", () => {
    const one = ended(1, "succeeded", 1_700_000_432);
    expect(finishedId(one)).toBe("WEB-24");
    expect(lasted(one)).toBe(432);
    expect(finishedKey(one)).toBe("finished:1");
  });
});

describe("what a step says in words", () => {
  const call = (tool: string, args: unknown) =>
    ({ tool, input: toolInput(tool, args), mcp: null }) as ToolCall;
  it("names the work, not the command", () => {
    expect(doingWords(call("read", { path: "/w/src/engine.rs" }))).toBe("reading engine.rs");
    expect(doingWords(call("edit", { path: "/w/src/engine.rs" }))).toBe("editing engine.rs");
    expect(doingWords(call("bash", { command: "cargo test -p cart discount" }))).toBe(
      "running the tests",
    );
    expect(doingWords(call("bash", { command: "cargo clippy -- -D warnings" }))).toBe(
      "checking the code",
    );
    expect(doingWords(call("bash", { command: 'rg -n "fn deliver" src' }))).toBe(
      "searching the code",
    );
    expect(doingWords(call("bash", { command: "git diff main...HEAD --stat" }))).toBe(
      "looking at the changes",
    );
    expect(doingWords(call("bash", { command: "make deploy" }))).toBe("running a command");
  });
});
