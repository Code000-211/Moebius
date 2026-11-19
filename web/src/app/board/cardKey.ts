/**
 * How a card is named: by its project's key and its number in the project (`WEB-24`). A card of a forge has the
 * forge's number besides (`WEB-24 · #17`), which is the number its issue has there. A server that sends no key (an older
 * one) leaves the plain `#17`.
 */
import type { Card } from "../../api/types/board";

/** The key of a project: a capital letter and one to nine capitals or digits. */
const KEY = /^[A-Z][A-Z0-9]{1,9}$/;

/** `WEB-24`, or `#17` for a card whose project has no key to name it by. */
export function cardKey(card: Pick<Card, "number" | "project_key" | "project_number">): string {
  return card.project_key
    ? `${card.project_key}-${card.project_number ?? card.number}`
    : `#${card.number}`;
}

/** The card's name, with the number of its issue on the forge beside the key when `forge` says it has one. */
export function cardName(
  card: Pick<Card, "number" | "project_key" | "project_number">,
  forge: boolean,
): string {
  return forge && card.project_key ? `${cardKey(card)} · #${card.number}` : cardKey(card);
}

/** The project key and number `text` names (`web-24` is `WEB` and 24), or `null` when it names none. */
export function parseCardKey(text: string): { key: string; number: number } | null {
  const [key, digits, ...more] = text.trim().toUpperCase().split("-");
  if (more.length > 0 || !key || !digits || !KEY.test(key) || !/^\d+$/.test(digits)) return null;
  return { key, number: Number(digits) };
}
