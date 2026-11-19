import { describe, expect, it } from "vitest";
import type { Comment } from "../../api/types/board";
import type { HistoryEntry } from "../../api/types/cards";
import { eventOf, foldLabel, timelineOf } from "./timeline";

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
    moved(60),
  ];

  it("puts comments and history in order and folds small changes", () => {
    const items = timelineOf(history, [comment(1, 30)]);
    expect(items.map((i) => i.type)).toEqual(["fold", "event", "comment", "event", "fold"]);
    const first = items[0];
    expect(first?.type === "fold" && first.events.length).toBe(3);
    const last = items[4];
    expect(last?.type === "fold" && last.events.length).toBe(2);
  });

  it("keeps one change alone as a line, not a fold", () => {
    expect(timelineOf([moved(1)], []).map((i) => i.type)).toEqual(["event"]);
  });

  it("does not fold a comment into the changes around it", () => {
    const items = timelineOf([moved(1), moved(3)], [comment(1, 2)]);
    expect(items.map((i) => i.type)).toEqual(["event", "comment", "event"]);
  });

  it("puts the history before a comment of the same second", () => {
    const items = timelineOf(
      [entry(5, "run_started", { run: 1, agent: "a", purpose: "p" })],
      [comment(1, 5)],
    );
    expect(items.map((i) => i.type)).toEqual(["event", "comment"]);
  });

  it("marks a run with its number and a refusal as such", () => {
    const run = eventOf(history[3] as HistoryEntry);
    expect(run).toMatchObject({ tone: "run", run: 7 });
    const refusal = eventOf(history[4] as HistoryEntry);
    expect(refusal).toMatchObject({ tone: "refusal", run: null });
    expect(refusal.text).toBe("no-force-push refused a tool call: git push --force");
    expect(eventOf(history[5] as HistoryEntry).tone).toBe("change");
  });

  it("says how many changes a fold holds", () => {
    expect(foldLabel(3)).toBe("3 changes");
  });
});
