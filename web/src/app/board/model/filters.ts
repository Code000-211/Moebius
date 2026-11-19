/**
 * The filters of the board: which values they take, the words that show them, and whether a card passes them.
 */
import type { Board, Card, InboxItem, Priority } from "../../../api/types/board";
import { inboxOf } from "./inbox";
import { cardStatus } from "./status";

export interface Filters {
  /** The empty string means every value. */
  column: string;
  agent: string;
  status: string;
  label: string;
  /** A priority (`urgent`, `high`, `medium`, `low`, `none`), or empty. */
  priority: string;
  /** `overdue`, `today`, `week` (within the next seven days), `any` or `none`, or empty. */
  due: string;
}

export const NO_FILTERS: Filters = {
  column: "",
  agent: "",
  status: "",
  label: "",
  priority: "",
  due: "",
};

export function hasFilters(filters: Filters): boolean {
  return Object.values(filters).some((value) => value !== "");
}

/** The priorities, most urgent first, each with the words that show it. */
export const PRIORITY_WORDS: Array<[Priority, string]> = [
  ["urgent", "Urgent"],
  ["high", "High"],
  ["medium", "Medium"],
  ["low", "Low"],
  ["none", "No priority"],
];

/** How urgent a priority is, 0 for the most urgent. */
export function priorityRank(priority: Priority): number {
  const rank = PRIORITY_WORDS.findIndex(([name]) => name === priority);
  return rank < 0 ? PRIORITY_WORDS.length : rank;
}

/** The choices of the due filter. */
export const DUE_WORDS: Array<[string, string]> = [
  ["overdue", "Overdue"],
  ["today", "Due today"],
  ["week", "Due within a week"],
  ["any", "Has a due date"],
  ["none", "No due date"],
];

/** Today as `YYYY-MM-DD` in the browser's own time zone. */
export function todayIso(now: Date = new Date()): string {
  const two = (n: number) => String(n).padStart(2, "0");
  return `${now.getFullYear()}-${two(now.getMonth() + 1)}-${two(now.getDate())}`;
}

/** `date` (`YYYY-MM-DD`) moved by `days`. */
function addDays(date: string, days: number): string {
  const [y = 0, m = 1, d = 1] = date.split("-").map(Number);
  return todayIso(new Date(y, m - 1, d + days));
}

/** Whether a due date passes the due filter; `today` is as `todayIso`. */
export function dueMatches(due: string | null, filter: string, today: string): boolean {
  switch (filter) {
    case "":
      return true;
    case "none":
      return due === null;
    case "any":
      return due !== null;
    case "overdue":
      return due !== null && due < today;
    case "today":
      return due === today;
    case "week":
      return due !== null && due >= today && due <= addDays(today, 7);
    default:
      return false;
  }
}

/** Whether a label is one of those that hold a card's priority, due date or estimate, which the card shows in its own way. */
export function isPlanningLabel(label: string): boolean {
  const lower = label.toLowerCase();
  return lower.startsWith("priority:") || lower.startsWith("due:") || lower.startsWith("estimate:");
}

/** The labels the issues of a board carry, sorted. */
export function labelsOf(board: Board): string[] {
  const names = new Set<string>();
  for (const card of board.cards) {
    for (const label of card.labels ?? []) if (!isPlanningLabel(label)) names.add(label);
  }
  return [...names].sort((a, b) => a.localeCompare(b));
}

/** The agents that run on the cards of a board, sorted. */
export function agentsOf(board: Board): string[] {
  const names = new Set<string>();
  for (const card of board.cards) if (card.run) names.add(card.run.agent);
  return [...names].sort((a, b) => a.localeCompare(b));
}

/** Whether a card passes the filters that look at the card itself: its agent, its status and its label. */
export function matchesCard(
  card: Card,
  filters: Filters,
  inbox: readonly InboxItem[],
  board: Board,
  today: string = todayIso(),
): boolean {
  if (filters.priority !== "" && card.priority !== filters.priority) return false;
  if (!dueMatches(card.due, filters.due, today)) return false;
  if (filters.agent !== "" && card.run?.agent !== filters.agent) return false;
  if (filters.status !== "" && cardStatus(card, inboxOf(inbox, card), board) !== filters.status) {
    return false;
  }
  if (filters.label !== "" && !(card.labels ?? []).includes(filters.label)) return false;
  return true;
}
