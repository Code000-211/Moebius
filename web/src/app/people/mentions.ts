/**
 * `@` completion in a text field: which handle is being typed at the caret, which people it can still be, and the
 * text with the choice written in. A handle starts after an `@` that begins a word (so an address such as
 * `me@example.com` is left alone) and is made of letters, digits, `-` and `_`, like the server's.
 */
import type { Person } from "../../api/types/server";

/** The most people offered at once. */
export const OFFERED = 6;

/** What is typed after an `@`: where the `@` is, and the letters after it up to the caret. */
export interface Typing {
  at: number;
  query: string;
}

/** The handle being typed at `caret`, or null when the caret is not right after `@` and a few handle characters. */
export function typing(text: string, caret: number): Typing | null {
  const before = text.slice(0, caret);
  const found = /(^|[^A-Za-z0-9_@])@([A-Za-z0-9_-]*)$/.exec(before);
  if (found === null) return null;
  const query = found[2] ?? "";
  return { at: caret - query.length - 1, query };
}

/** The people whose handle or name starts with what is typed, enabled ones only, handles first, at most `OFFERED`. */
export function offered(people: readonly Person[], query: string): Person[] {
  const q = query.toLowerCase();
  const live = people.filter((person) => !person.disabled);
  const byHandle = live.filter((person) => person.handle.toLowerCase().startsWith(q));
  const byName = live.filter(
    (person) =>
      !byHandle.includes(person) &&
      person.name
        .toLowerCase()
        .split(/\s+/)
        .some((w) => w.startsWith(q)),
  );
  return [...byHandle, ...byName].slice(0, OFFERED);
}

/**
 * The text with the `@` at `at` and what follows it up to `caret` replaced by `@handle`, and a space after it unless
 * one follows already; and the new caret, right after the handle's space.
 */
export function completed(
  text: string,
  at: number,
  caret: number,
  handle: string,
): { text: string; caret: number } {
  const rest = text.slice(caret);
  const written = /^\s/.test(rest) ? `@${handle}` : `@${handle} `;
  return { text: text.slice(0, at) + written + rest, caret: at + `@${handle} `.length };
}
