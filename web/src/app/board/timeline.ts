/**
 * A card's history as one timeline: the comments of people and the entries of `GET /v1/cards/{id}/history` put in
 * order. A move is a small line of its own; a run is one line that holds what its start and its end said (a coding
 * run, a review or another run, told apart by the run's purpose); a refusal of a rule is a line; the other small
 * changes that follow one another are folded into one line ("3 changes"). What waits for a person comes last, as one
 * block, from the item of the Inbox the card has.
 */
import { classOf, type HasColumns } from "../../api/columns";
import type { Comment, InboxItem } from "../../api/types/board";
import type { HistoryEntry } from "../../api/types/cards";
import { describeHistory } from "./model";

/** A line of the timeline: what happened and when. */
export interface TimelineEvent {
  at: number;
  text: string;
  /** How it is marked: a refusal of a rule, a move to a column, or a plain change. */
  tone: "refusal" | "move" | "change";
  /** The column a move left. */
  from?: string;
  /** The column a move went to. */
  to?: string;
  /** The built-in column the one a move left counts as; set when the project's columns are known. */
  fromClass?: string;
  /** The built-in column the one a move went to counts as. */
  toClass?: string;
  /** What was said about the move. */
  note?: string;
}

/** What a run is called on the timeline: the coding run and the review are told apart; any other is named by its purpose. */
export type RunKind = "code" | "review" | "other";

/** A run of the card, from its start and its end. */
export interface TimelineRun {
  run: number;
  agent: string;
  purpose: string;
  kind: RunKind;
  startedAt: number;
  /** When it ended; null while it runs. */
  endedAt: number | null;
  /** How it ended (`done`, `failed`, `stopped`…); null while it runs. */
  status: string | null;
}

export type TimelineItem =
  | { type: "comment"; comment: Comment }
  | { type: "move"; event: TimelineEvent }
  | { type: "run"; run: TimelineRun }
  | { type: "event"; event: TimelineEvent }
  | { type: "fold"; events: TimelineEvent[] }
  | { type: "needs-you"; item: InboxItem };

/** A decision about a tool call that said no: a rule (or a person) refused it. */
function isRefusal(entry: HistoryEntry): boolean {
  return (
    entry.kind === "decided" && entry.data.kind === "tool_call" && entry.data.choice === "deny"
  );
}

const text = (value: unknown): string => (typeof value === "string" ? value : "");

/** Which of the card's runs this purpose is: the review of a change (`review`, `review-spec`…) or the code run. */
export function runKindOf(purpose: string): RunKind {
  if (purpose === "code") return "code";
  if (purpose === "review" || purpose.startsWith("review-")) return "review";
  return "other";
}

/** What a run is called: "Code run", "Review", or the purpose in a word such as "Plan run". */
export function runLabel(run: TimelineRun): string {
  if (run.kind === "code") return "Code run";
  if (run.kind === "review") return "Review";
  const word = run.purpose.replaceAll("-", " ");
  return `${word.charAt(0).toUpperCase()}${word.slice(1)} run`.trim();
}

/** The entry as a line of the timeline. */
export function eventOf(entry: HistoryEntry, columns?: HasColumns): TimelineEvent {
  if (entry.kind === "moved") {
    const from = text(entry.data.from);
    const to = text(entry.data.to);
    return {
      fromClass: classOf(columns, from),
      toClass: classOf(columns, to),
      at: entry.at,
      text: describeHistory(entry),
      tone: "move",
      from,
      to,
      note: text(entry.data.note),
    };
  }
  if (isRefusal(entry)) {
    const reason = text(entry.data.reason);
    const decider = text(entry.data.decider) || "A rule";
    return {
      at: entry.at,
      text: reason ? `${decider} refused a tool call: ${reason}` : `${decider} refused a tool call`,
      tone: "refusal",
    };
  }
  return { at: entry.at, text: describeHistory(entry), tone: "change" };
}

/**
 * The comments and the history in order of time; of the same second the history comes first, in the order the server
 * sent it. A run's start and end are one item, where it started. Changes that follow one another, with nothing else
 * between them, become one fold; one change alone stays a line. The item of the Inbox that waits for a person ends it.
 */
export function timelineOf(
  history: HistoryEntry[],
  comments: Comment[],
  needsYou?: InboxItem,
  /** The columns of the card's project: a move between custom columns is read by their classes. */
  columns?: HasColumns,
): TimelineItem[] {
  const ended = new Map<number, HistoryEntry>();
  for (const entry of history) {
    if (entry.kind === "run_ended" && typeof entry.data.run === "number") {
      ended.set(entry.data.run, entry);
    }
  }
  const merged: Array<{ at: number; item: TimelineItem }> = [];
  for (const entry of history) {
    if (entry.kind === "run_ended") continue;
    if (entry.kind === "run_started" && typeof entry.data.run === "number") {
      const end = ended.get(entry.data.run);
      const purpose = text(entry.data.purpose);
      const run: TimelineRun = {
        run: entry.data.run,
        agent: text(entry.data.agent),
        purpose,
        kind: runKindOf(purpose),
        startedAt: entry.at,
        endedAt: end?.at ?? null,
        status: end ? text(end.data.status) : null,
      };
      merged.push({ at: entry.at, item: { type: "run", run } });
      continue;
    }
    const event = eventOf(entry, columns);
    merged.push({ at: entry.at, item: { type: event.tone === "move" ? "move" : "event", event } });
  }
  for (const comment of comments)
    merged.push({ at: comment.at, item: { type: "comment", comment } });
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
  if (needsYou) items.push({ type: "needs-you", item: needsYou });
  return items;
}

/** What a fold says of itself: how many changes it holds. */
export function foldLabel(count: number): string {
  return `${count} changes`;
}

/** How long a run took, such as `4m 12s`; null while it runs. */
export function runDuration(run: TimelineRun): string | null {
  if (run.endedAt === null) return null;
  const seconds = Math.max(0, run.endedAt - run.startedAt);
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3_600) return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
  return `${Math.floor(seconds / 3_600)}h ${Math.floor((seconds % 3_600) / 60)}m`;
}

/** The classes of the columns a card is sent back from when its review or its merge asks for more work. */
const SENT_BACK_FROM = ["In review", "Ready to merge"];
const SENT_BACK_TO = ["In progress", "Not started"];

/** What a move says of itself: "Moved to In progress", or "Sent back to In progress" when a review turned it back. */
export function moveLabel(event: TimelineEvent): string {
  const back =
    event.note !== undefined &&
    event.note !== "" &&
    SENT_BACK_FROM.includes(event.fromClass ?? event.from ?? "") &&
    SENT_BACK_TO.includes(event.toClass ?? event.to ?? "");
  return `${back ? "Sent back" : "Moved"} to ${event.to ?? ""}`;
}

/** A reason that fits on the line of the move: one short line with no Markdown in it. */
export function isShortNote(note: string): boolean {
  return note.length <= 80 && !/[\n`*_#>[\]|]|^\s*[-+\d]/.test(note);
}
