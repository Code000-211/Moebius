import { describe, expect, it } from "vitest";
import type { Comment } from "../../api/types/board";
import type { HistoryEntry } from "../../api/types/cards";
import { eventOf, foldLabel, runDuration, runKindOf, runLabel, timelineOf } from "./timeline";

const entry = (at: number, kind: string, data: Record<string, unknown> = {}): HistoryEntry => ({
  at,
  kind,
  data,
});
const comment = (id: number, at: number): Comment => ({
  id,
  card_id: 1,
  author: "tom",
  body: `Comment ${id}`,
  at,
});
const moved = (at: number) => entry(at, "moved", { from: "A", to: "B", note: null });

describe("the timeline of a card", () => {
  const history = [
    moved(10),
    entry(11, "inbox_added", { item: 1, kind: "approve_issue", title: "Approve" }),
    entry(12, "inbox_resolved", { item: 1, kind: "approve_issue", title: "Approve" }),
    entry(20, "run_started", { run: 7, agent: "Coder", purpose: "code" }),
    entry(35, "run_ended", { run: 7, status: "done" }),
    entry(40, "decided", {
      judgement: 1,
      kind: "tool_call",
      choice: "deny",
      decider: "no-force-push",
      reason: "git push --force",
      outcome: null,
    }),
    entry(50, "decided", {
      judgement: 2,
      kind: "merge",
      choice: "merge",
      decider: "person",
      reason: "ok",
      outcome: null,
    }),
    entry(55, "run_started", { run: 8, agent: "Reviewer", purpose: "review-spec" }),
    moved(60),
  ];

  it("puts comments and history in order; a move and a run are items of their own", () => {
    const items = timelineOf(history, [comment(1, 30)]);
    expect(items.map((i) => i.type)).toEqual([
      "move",
      "fold",
      "run",
      "comment",
      "event",
      "event",
      "run",
      "move",
    ]);
    const fold = items[1];
    expect(fold?.type === "fold" && fold.events.length).toBe(2);
  });

  it("makes one item of a run's start and end, and tells a coding run from a review", () => {
    const runs = timelineOf(history, []).flatMap((i) => (i.type === "run" ? [i.run] : []));
    expect(runs.map((r) => [r.run, r.kind, runLabel(r), r.status, runDuration(r)])).toEqual([
      [7, "code", "Code run", "done", "15s"],
      [8, "review", "Review", null, null],
    ]);
    expect(
      timelineOf(history, []).some((i) => i.type === "event" && /Run 7 done/.test(i.event.text)),
    ).toBe(false);
  });

  it("names a run of another purpose by it", () => {
    const [item] = timelineOf(
      [entry(1, "run_started", { run: 1, agent: "a", purpose: "plan" })],
      [],
    );
    expect(item?.type === "run" && runLabel(item.run)).toBe("Plan run");
    expect(runKindOf("review-select")).toBe("review");
    expect(runKindOf("code")).toBe("code");
  });

  it("says how long a run took", () => {
    const run = (end: number) => ({
      run: 1,
      agent: "a",
      purpose: "code",
      kind: "code" as const,
      startedAt: 0,
      endedAt: end,
      status: "done",
    });
    expect(runDuration(run(252))).toBe("4m 12s");
    expect(runDuration(run(7500))).toBe("2h 5m");
  });

  it("ends with one needs-you item when the card waits for a person, and with none otherwise", () => {
    const waiting = {
      id: 3,
      kind: "approve_issue",
      card_id: 1,
      title: "Approve",
      data: {},
      resolved_at: null,
      at: 70,
      assignees: [],
    };
    const items = timelineOf([moved(1)], [], waiting);
    expect(items.map((i) => i.type)).toEqual(["move", "needs-you"]);
    expect(timelineOf([moved(1)], []).some((i) => i.type === "needs-you")).toBe(false);
  });

  it("keeps one change alone as a line, not a fold", () => {
    expect(timelineOf([entry(1, "inbox_added", { title: "x" })], []).map((i) => i.type)).toEqual([
      "event",
    ]);
  });

  it("does not fold a comment into the changes around it", () => {
    const change = (at: number) => entry(at, "inbox_added", { title: "x" });
    const items = timelineOf([change(1), change(3)], [comment(1, 2)]);
    expect(items.map((i) => i.type)).toEqual(["event", "comment", "event"]);
  });

  it("puts the history before a comment of the same second", () => {
    const items = timelineOf(
      [entry(5, "run_started", { run: 1, agent: "a", purpose: "p" })],
      [comment(1, 5)],
    );
    expect(items.map((i) => i.type)).toEqual(["run", "comment"]);
  });

  it("marks a refusal and a move as such", () => {
    const refusal = eventOf(history[5] as HistoryEntry);
    expect(refusal).toMatchObject({ tone: "refusal" });
    expect(refusal.text).toBe("no-force-push refused a tool call: git push --force");
    expect(eventOf(history[6] as HistoryEntry).tone).toBe("change");
    expect(eventOf(history[0] as HistoryEntry)).toMatchObject({ tone: "move", to: "B" });
  });

  it("says how many changes a fold holds", () => {
    expect(foldLabel(3)).toBe("3 changes");
  });
});
