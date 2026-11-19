/**
 * A card's history as one timeline: the comments of people and the entries of `GET /v1/cards/{id}/history` put in
 * order, with the small changes that follow one another folded into one line ("3 changes"). Only a comment has a
 * frame of its own; a run and a refusal of a rule are lines that say more than a change, so they are never folded.
 */
import type { Comment } from "../../api/types/board";
import type { HistoryEntry } from "../../api/types/cards";
import { describeHistory } from "./model";

/** A line of the timeline: what happened, when, and the run it opens, if it was one. */
export interface TimelineEvent {
  at: number;
  text: string;
  /** How it is marked: a run, a refusal of a rule, or a plain change. */
  tone: "run" | "refusal" | "change";
  /** The run this line opens the page of. */
  run: number | null;
}

export type TimelineItem =
  | { type: "comment"; comment: Comment }
  | { type: "event"; event: TimelineEvent }
  | { type: "fold"; events: TimelineEvent[] };

/** A decision about a tool call that said no: a rule (or a person) refused it. */
function isRefusal(entry: HistoryEntry): boolean {
  return (
    entry.kind === "decided" && entry.data.kind === "tool_call" && entry.data.choice === "deny"
  );
}

function runOf(entry: HistoryEntry): number | null {
  const run = entry.data.run;
  return typeof run === "number" ? run : null;
}

/** The entry as a line of the timeline. */
export function eventOf(entry: HistoryEntry): TimelineEvent {
  if (entry.kind === "run_started") {
    return { at: entry.at, text: describeHistory(entry), tone: "run", run: runOf(entry) };
  }
  if (isRefusal(entry)) {
    const reason = typeof entry.data.reason === "string" ? entry.data.reason : "";
    const decider = typeof entry.data.decider === "string" ? entry.data.decider : "A rule";
    return {
      at: entry.at,
      text: reason ? `${decider} refused a tool call: ${reason}` : `${decider} refused a tool call`,
      tone: "refusal",
      run: null,
    };
  }
  return { at: entry.at, text: describeHistory(entry), tone: "change", run: null };
}

/**
 * The comments and the history in order of time; of the same second the history comes first, in the order the server
 * sent it. Changes that follow one another, with nothing else between them, become one fold; one change alone stays
 * a line.
 */
export function timelineOf(history: HistoryEntry[], comments: Comment[]): TimelineItem[] {
  const merged: Array<{ at: number; item: TimelineItem }> = [
    ...history.map((entry) => ({
      at: entry.at,
      item: { type: "event", event: eventOf(entry) } as TimelineItem,
    })),
    ...comments.map((comment) => ({
      at: comment.at,
      item: { type: "comment", comment } as TimelineItem,
    })),
  ];
  // Array.prototype.sort is stable, so equal times keep the history first and each list in its own order.
  merged.sort((a, b) => a.at - b.at);
  const items: TimelineItem[] = [];
  let pending: TimelineEvent[] = [];
  const flush = () => {
    if (pending.length === 1) items.push({ type: "event", event: pending[0] as TimelineEvent });
    else if (pending.length > 1) items.push({ type: "fold", events: pending });
    pending = [];
  };
  for (const { item } of merged) {
    if (item.type === "event" && item.event.tone === "change") {
      pending.push(item.event);
      continue;
    }
    flush();
    items.push(item);
  }
  flush();
  return items;
}

/** What a fold says of itself: how many changes it holds. */
export function foldLabel(count: number): string {
  return `${count} changes`;
}
