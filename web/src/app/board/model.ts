/**
 * What the board page decides, as plain functions: which columns a board has, which cards each shows after the
 * filters, what state a card is in, what a drop of a card on a column means (and why it may be refused), and the
 * words and links of a card. `screens/board.rs` of the desktop app is the model: the columns, the order, the ten cards
 * of Done and the card the dock shows without a choice are the same.
 */
import type { Board, Card, Change, InboxItem, Repository } from "../../api/types/board";
import type { HistoryEntry } from "../../api/types/cards";
import { plural } from "../../ui/lib/format";
import type { Tone } from "../../ui/page/Status";

/** The columns of the default workflow, left to right. */
export const COLUMNS = [
  "Backlog",
  "Not started",
  "In progress",
  "In review",
  "Ready to merge",
  "Done",
] as const;

/** How many cards Done lists, the highest numbers first, because it keeps growing. */
export const DONE_SHOWN = 10;

/** The names of the columns a board shows: Backlog only while it holds a card, and any other column a card is in, last. */
export function boardColumns(board: Board): string[] {
  const columns: string[] = COLUMNS.filter(
    (name) => name !== "Backlog" || board.cards.some((card) => card.column === "Backlog"),
  );
  for (const card of board.cards) {
    if (!columns.includes(card.column)) columns.push(card.column);
  }
  return columns;
}

/** The kinds of inbox item that stop a card for a person. */
const STUCK = "stuck";
const CHANGE_CLOSED = "change_closed";
const READY_TO_MERGE = "ready_to_merge";
const APPROVE_ISSUE = "approve_issue";

/** The open inbox item of a card, if it has one. */
export function inboxOf(inbox: readonly InboxItem[], card: Card): InboxItem | undefined {
  return inbox.find((item) => item.card_id === card.id && item.resolved_at === null);
}

/** Whether a card is being worked on: its latest run starts or runs. */
export function isLive(card: Card): boolean {
  return card.run?.status === "starting" || card.run?.status === "running";
}

export type CardStatus = "live" | "needs-you" | "stuck" | "blocked" | "waiting" | "idle";

export const STATUS_LABELS: Record<CardStatus, string> = {
  live: "Live",
  "needs-you": "Your turn",
  stuck: "Stuck",
  blocked: "Blocked",
  waiting: "Waiting",
  idle: "Idle",
};

/** What a card is doing, the first that applies: a run going, a person asked, stuck, blocked, waiting, or nothing. */
export function cardStatus(card: Card, item: InboxItem | undefined, board: Board): CardStatus {
  if (isLive(card)) return "live";
  if (item?.kind === STUCK) return "stuck";
  if (item) return "needs-you";
  if (card.blocked_by.length > 0) return "blocked";
  if (board.waiting[String(card.id)] !== undefined) return "waiting";
  return "idle";
}

export interface Filters {
  /** The empty string means every value. */
  column: string;
  agent: string;
  status: string;
}

export const NO_FILTERS: Filters = { column: "", agent: "", status: "" };

export function hasFilters(filters: Filters): boolean {
  return filters.column !== "" || filters.agent !== "" || filters.status !== "";
}

/** The agents that run on the cards of a board, sorted. */
export function agentsOf(board: Board): string[] {
  const names = new Set<string>();
  for (const card of board.cards) if (card.run) names.add(card.run.agent);
  return [...names].sort((a, b) => a.localeCompare(b));
}

function matches(card: Card, filters: Filters, inbox: readonly InboxItem[], board: Board): boolean {
  if (filters.column !== "" && card.column !== filters.column) return false;
  if (filters.agent !== "" && card.run?.agent !== filters.agent) return false;
  if (filters.status !== "" && cardStatus(card, inboxOf(inbox, card), board) !== filters.status) {
    return false;
  }
  return true;
}

export interface ColumnCards {
  /** The name of the column. */
  column: string;
  /** The cards shown, in order. */
  cards: Card[];
  /** How many cards the column has after the filters (Done shows only some of them). */
  total: number;
}

/** The columns of a board with the cards each shows: filtered, ordered by number, Done newest first and cut. */
export function groupByColumn(
  board: Board,
  inbox: readonly InboxItem[],
  filters: Filters,
): ColumnCards[] {
  const kept = board.cards.filter((card) => matches(card, filters, inbox, board));
  return boardColumns(board)
    .filter((column) => filters.column === "" || column === filters.column)
    .map((column) => {
      const cards = kept
        .filter((card) => card.column === column)
        .sort((a, b) => a.number - b.number);
      if (column === "Done") cards.reverse();
      return {
        column,
        cards: column === "Done" ? cards.slice(0, DONE_SHOWN) : cards,
        total: cards.length,
      };
    });
}

/**
 * The card the dock shows. A card chosen with `selected` (a card id, or "none" for a dock closed on purpose) wins;
 * without a choice it is the first live card, else the first that waits for a person, else the first not done.
 */
export function inspectedCard(
  board: Board,
  inbox: readonly InboxItem[],
  selected: string | null,
): Card | undefined {
  if (selected === "none") return undefined;
  if (selected !== null) {
    const chosen = board.cards.find((card) => String(card.id) === selected);
    if (chosen) return chosen;
  }
  return (
    board.cards.find(isLive) ??
    board.cards.find((card) => inboxOf(inbox, card) !== undefined) ??
    board.cards.find((card) => card.column !== "Done")
  );
}

/** What a drop of a card on a column means. */
export type Drop =
  | { kind: "none" }
  | { kind: "approve" }
  | { kind: "retry" }
  | { kind: "merge"; change: number }
  /** Sending a change back needs a reason, which the dock asks for. */
  | { kind: "reject"; change: number }
  | { kind: "refused"; reason: string };

function changeOfItem(item: InboxItem): number | null {
  const change = item.data.change;
  return typeof change === "number" ? change : null;
}

/**
 * What dropping `card` on the column `to` does. A person decides only what waits in the inbox: an issue to approve
 * goes to Not started, a change to merge goes to Done or back to In progress (rejected), and a stuck card, or one whose pull
 * request was closed, goes to In progress (retried). Any other drop is refused with the reason, since the board moves
 * every other card itself when its column's script says so.
 */
export function dropRule(card: Card, to: string, item: InboxItem | undefined): Drop {
  const kind = item?.kind;
  if (kind === READY_TO_MERGE && item) {
    const change = changeOfItem(item);
    if (to === "Done" || to === "In progress") {
      if (change === null) {
        return { kind: "refused", reason: "This card has no change to decide on yet." };
      }
      return to === "Done" ? { kind: "merge", change } : { kind: "reject", change };
    }
  }
  if (kind === APPROVE_ISSUE && to === "Not started") return { kind: "approve" };
  if ((kind === STUCK || kind === CHANGE_CLOSED) && to === "In progress") return { kind: "retry" };
  if (card.column === to) return { kind: "none" };
  if (isLive(card)) {
    return {
      kind: "refused",
      reason: "Moebius is working on this card. It moves when its run ends.",
    };
  }
  switch (kind) {
    case READY_TO_MERGE:
      return {
        kind: "refused",
        reason:
          "This change waits for you: drop it on Done to merge it or on In progress to send it back.",
      };
    case APPROVE_ISSUE:
      return {
        kind: "refused",
        reason: "This issue waits for your approval: drop it on Not started to approve it.",
      };
    case STUCK:
    case CHANGE_CLOSED:
      return { kind: "refused", reason: "This card is stuck: drop it on In progress to retry it." };
    default:
      return {
        kind: "refused",
        reason: `Nobody decides this card now. The board moves it from ${card.column} when the column's script says so.`,
      };
  }
}

/** A number of tokens as the board writes it: `96k tok`, `1.2M tok`. */
export function tokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M tok`;
  if (n >= 1_000) return `${Math.floor(n / 1_000)}k tok`;
  return `${n} tok`;
}

/**
 * The small line of a card under its number: in coding and review the agent and how far its rounds are, for a change
 * that waits to merge the rounds and what it cost; null when there is nothing to say.
 */
export function cardDetail(card: Card, roundLimit: number): string | null {
  if (card.column === "In progress" || card.column === "In review") {
    const parts: string[] = [];
    if (card.run) parts.push(card.run.agent);
    if (card.rounds > 0) {
      parts.push(roundLimit > 0 ? `round ${card.rounds} of ${roundLimit}` : `round ${card.rounds}`);
    }
    return parts.length > 0 ? parts.join(" · ") : null;
  }
  if (card.column === "Ready to merge") {
    return `${plural(card.rounds, "round", "rounds")} · ${tokens(card.tokens)}`;
  }
  return null;
}

/** The address of a card's issue on its forge; only GitHub's address is known (the others' hosts are not). */
export function issueUrl(repository: Repository, card: Card): string | null {
  if (repository.forge === "github" && repository.github) {
    return `https://github.com/${repository.github}/issues/${card.number}`;
  }
  return null;
}

/** The address of a pull request on GitHub; the other forges' hosts are not known. */
export function pullUrl(repository: Repository, pullRequest: number): string | null {
  if (repository.forge === "github" && repository.github) {
    return `https://github.com/${repository.github}/pull/${pullRequest}`;
  }
  return null;
}

/** The change of a card: its open one, else the latest it ever had. */
export function changeOf(board: Board, card: Card): Change | undefined {
  if (card.change) {
    const open = board.changes.find((change) => change.id === card.change?.id);
    if (open) return open;
  }
  return board.changes
    .filter((change) => change.card_id === card.id)
    .sort((a, b) => b.id - a.id)[0];
}

/**
 * What a card waits for, as one line: the reason its column script gave, the rules that refused it, both, or
 * "Nothing". A card with nothing to say is not stuck, and an empty row read as a missing fact is worse than the word.
 */
export function waitingLine(reason: string | undefined, refusals: number): string {
  const parts = [reason];
  if (refusals > 0) parts.push(`${plural(refusals, "rule", "rules")} broken`);
  const line = parts.filter((part) => part !== undefined && part !== "").join(" · ");
  return line === "" ? "Nothing" : line;
}

/** What a person reads of one entry of a card's history. */
export function describeHistory(entry: HistoryEntry): string {
  const data = entry.data;
  const word = (key: string): string => {
    const value = data[key];
    return typeof value === "string"
      ? value
      : value === undefined || value === null
        ? ""
        : String(value);
  };
  switch (entry.kind) {
    case "moved":
      return typeof data.note === "string" && data.note !== ""
        ? `Moved from ${word("from")} to ${word("to")}: ${data.note}`
        : `Moved from ${word("from")} to ${word("to")}`;
    case "run_started":
      return `Run ${word("run")} started: ${word("agent")}, ${word("purpose")}`;
    case "run_ended":
      return `Run ${word("run")} ${word("status")}`;
    case "decided":
      return `${word("decider")} decided ${word("choice")}: ${word("reason")}`;
    case "attempt_judged":
      return `Attempt ${word("run")} ${word("result")}${word("output") ? `: ${word("output")}` : ""}`;
    case "attempts_settled":
      return data.picked === null || data.picked === undefined
        ? "No attempt passed"
        : `Attempt ${word("picked")} was kept${word("reason") ? `: ${word("reason")}` : ""}`;
    case "inbox_added":
      return `Asked you: ${word("title")}`;
    case "inbox_resolved":
      return `Answered: ${word("title")}`;
    default:
      return entry.kind.replaceAll("_", " ");
  }
}

export interface CardBadge {
  name: string;
  /** The state the pill's colour stands for, in the names of the shared pill (`ui/page/Status`). */
  tone: Tone;
}

/** The badges of a card: what runs, what it costs, what stops it, how its change stands, who it waits for. */
export function cardBadges(card: Card, status: CardStatus, next: boolean): CardBadge[] {
  const badges: CardBadge[] = [];
  if (status === "live") badges.push({ name: "Live", tone: "progress" });
  if (card.tokens > 0 && (card.column === "In progress" || card.column === "In review")) {
    badges.push({ name: tokens(card.tokens), tone: "neutral" });
  }
  if (card.blocked_by.length > 0) {
    const numbers = card.blocked_by.map((n) => `#${n}`);
    const name =
      numbers.length <= 2
        ? `Blocked by ${numbers.join(", ")}`
        : `Blocked by ${numbers[0]} and ${numbers.length - 1} more`;
    badges.push({ name, tone: "error" });
  }
  if (card.refusals > 0) {
    badges.push({
      name: plural(card.refusals, "rule broken", "rules broken"),
      tone: "error",
    });
  }
  switch (card.change?.checks) {
    case "pending":
      badges.push({ name: "CI running", tone: "progress" });
      break;
    case "success":
      badges.push({ name: "Green", tone: "complete" });
      break;
    case "failure":
      badges.push({ name: "CI failed", tone: "error" });
      break;
  }
  if (status === "stuck") badges.push({ name: "Stuck", tone: "error" });
  if (status === "needs-you") badges.push({ name: "Your turn", tone: "attention" });
  if (next) badges.push({ name: "Next", tone: "neutral" });
  return badges;
}

/** The card the board starts next: the first in Not started that nothing blocks. */
export function nextCard(cards: readonly Card[]): number | undefined {
  return cards.find((card) => card.blocked_by.length === 0)?.id;
}
