/**
 * Links in the wiki's text, on the page's side: `[[Page name]]` made a link to the page the server says it leads to,
 * and what completes while a person types `[[` or the start of a card's name. The server decides which page a name
 * leads to (`docs/reference/desktop-api.md`, "Links, backlinks and renames"); this only draws and completes.
 */
import type { PageInfo, PageLink } from "../../api/types/wiki";

/** Code first, so that a link inside it stays text; then `[[target|label]]`. */
const LINK = /(```[\s\S]*?```|~~~[\s\S]*?~~~|`+[^`\n]*`+)|\[\[([^[\]|\n]+)(?:\|([^[\]\n]*))?\]\]/g;

/** The address of a page on the Wiki page. */
export function wikiAddress(path: string): string {
  return `/wiki?page=${encodeURIComponent(path)}`;
}

/** `text` with each `[[link]]` that leads to a page made a Markdown link to it; one that leads nowhere stays as written. */
export function linkPages(text: string, links: readonly PageLink[]): string {
  const leads = new Map(links.map((link) => [link.link, link.path]));
  return text.replace(
    LINK,
    (whole, code, target: string | undefined, label: string | undefined) => {
      if (code !== undefined || target === undefined) return whole;
      const path = leads.get(target.trim());
      if (!path) return whole;
      const shown = (label ?? target).trim() || target.trim();
      return `[${shown.replace(/([[\]\\])/g, "\\$1")}](${wikiAddress(path).replace(/\)/g, "%29")})`;
    },
  );
}

/** What is being completed at the caret: a page after `[[`, or a card once its key and hyphen are typed. */
export interface Completion {
  kind: "page" | "card";
  /** What has been typed of it. */
  query: string;
  /** Where in the text what is replaced starts: after the `[[` for a page, at the name for a card. */
  from: number;
}

/** The completion under `caret` in `text`, or null when the caret is in no link or card name. */
export function completionAt(text: string, caret: number): Completion | null {
  const line = text.slice(text.lastIndexOf("\n", caret - 1) + 1, caret);
  const open = line.lastIndexOf("[[");
  if (open >= 0 && !/[\]|]/.test(line.slice(open + 2))) {
    return { kind: "page", query: line.slice(open + 2), from: caret - line.length + open + 2 };
  }
  const key = /(?:^|[^\w/#&.-])([A-Z][A-Z0-9]{1,8}-[0-9]*)$/.exec(line);
  if (key?.[1]) return { kind: "card", query: key[1], from: caret - key[1].length };
  return null;
}

/** How many choices are offered. */
export const CHOICES = 8;

/** The pages `query` completes to: those whose name or path holds it, a name that starts with it first. */
export function pageChoices(pages: readonly PageInfo[], query: string): PageInfo[] {
  const wanted = query.trim().toLowerCase();
  const rank = (page: PageInfo) =>
    page.name.toLowerCase().startsWith(wanted)
      ? 0
      : page.path.toLowerCase().includes(wanted)
        ? 1
        : 2;
  return pages
    .filter((page) => rank(page) < 2)
    .sort((a, b) => rank(a) - rank(b) || (a.path < b.path ? -1 : a.path > b.path ? 1 : 0))
    .slice(0, CHOICES);
}

/** The text a page is linked by: its name when no other page has it, else its path. */
export function linkText(page: PageInfo, pages: readonly PageInfo[]): string {
  const same = pages.filter((other) => other.name.toLowerCase() === page.name.toLowerCase());
  return same.length === 1 ? page.name : page.path;
}

/** A card that may be offered. */
export interface CardChoice {
  key: string;
  title: string;
}

/** The cards whose name starts with `query`, in the order given. */
export function cardChoices(cards: readonly CardChoice[], query: string): CardChoice[] {
  const wanted = query.toUpperCase();
  return cards.filter((card) => card.key.startsWith(wanted)).slice(0, CHOICES);
}

/** `text` with what was typed since `from` replaced by `insert`, and where the caret goes: after the closing `]]` of a page. */
export function complete(
  text: string,
  caret: number,
  completion: Completion,
  insert: string,
): { text: string; caret: number } {
  if (completion.kind === "card") {
    return {
      text: text.slice(0, completion.from) + insert + text.slice(caret),
      caret: completion.from + insert.length,
    };
  }
  const end = text.startsWith("]]", caret) ? caret + 2 : caret;
  return {
    text: `${text.slice(0, completion.from)}${insert}]]${text.slice(end)}`,
    caret: completion.from + insert.length + 2,
  };
}
