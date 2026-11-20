/**
 * What the palette is made of, without a component: a command (what it is called, the key that does it on the card in
 * focus, what it runs), the focus a page registers (a label and its commands), how a query matches, and the places a
 * person was at lately, kept for this browser.
 */
import { readText, writeText } from "../../storage";

export interface PaletteCommand {
  /** Stable name of the command, for the list's keys and the tests. */
  id: string;
  label: string;
  /** The single key that does the command while its focus is in the page and no field has the keys. */
  key?: string;
  /** The command opens a dialog of its own, so the palette must not hand the focus back to where it was. */
  opensDialog?: boolean;
  run: () => void;
}

/** What a page has in focus (a card), with the commands that belong to it. */
export interface PaletteFocus {
  label: string;
  commands: readonly PaletteCommand[];
}

/** Every word of the query is somewhere in the text (case does not matter); an empty query matches everything. */
export function matches(query: string, text: string): boolean {
  const haystack = text.toLowerCase();
  return query
    .toLowerCase()
    .split(/\s+/)
    .filter((word) => word !== "")
    .every((word) => haystack.includes(word));
}

/**
 * How well `text` answers the query, 0 for no match: 3 when every word typed is a whole word of the text, 2 when each
 * starts a word, 1 when it is only somewhere inside one (so "tom" finds "Automations" below the person Tom).
 */
export function rank(query: string, text: string): number {
  const words = query
    .toLowerCase()
    .split(/\s+/)
    .filter((word) => word !== "");
  if (!matches(query, text)) return 0;
  const parts = text.toLowerCase().split(/[^\p{L}\p{N}]+/u);
  if (words.every((word) => parts.includes(word))) return 3;
  if (words.every((word) => parts.some((part) => part.startsWith(word)))) return 2;
  return 1;
}

/** The items that match the query, the better matches first and the rest in their order. */
export function ranked<T>(query: string, items: readonly T[], text: (item: T) => string): T[] {
  return items
    .map((item, at) => ({ item, at, score: rank(query, text(item)) }))
    .filter((entry) => entry.score > 0)
    .sort((a, b) => b.score - a.score || a.at - b.at)
    .map((entry) => entry.item);
}

const RECENT_KEY = "moebius.recent";
const RECENT_KEPT = 6;

/** The paths a person was at lately, the latest first. */
export function readRecent(): string[] {
  try {
    const kept: unknown = JSON.parse(readText(RECENT_KEY) ?? "[]");
    return Array.isArray(kept) ? kept.filter((p): p is string => typeof p === "string") : [];
  } catch {
    return [];
  }
}

/** Puts `path` first among the places seen lately, once. */
export function noteRecent(path: string): void {
  const next = [path, ...readRecent().filter((p) => p !== path)].slice(0, RECENT_KEPT);
  writeText(RECENT_KEY, JSON.stringify(next));
}
