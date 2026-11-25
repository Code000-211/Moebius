//! What the My issues page shows: the cards given to a person, grouped by status, as the flat list `VirtualList` draws.

import { cardClass, isDone } from "../../api/columns";
import type { Card } from "../../api/types/board";
import type { StatusState } from "../../ui/page/StatusIcon";
import type { ListItem } from "../../ui/page/VirtualList";
import { cardKey } from "../board/cardKey";

/** The groups, in the order the page shows them: what is being worked on first, what is finished last. */
export const STATUS_GROUPS = ["In progress", "In review", "Not started", "Done"] as const;
export type StatusGroup = (typeof STATUS_GROUPS)[number];

/** The group a card is in, from the column it stands in; a column of a workflow of the person's own is "Not started". */
export function groupOf(card: Pick<Card, "column" | "column_class" | "closed">): StatusGroup {
  if (card.closed || isDone(card)) return "Done";
  if (cardClass(card) === "In progress") return "In progress";
  if (cardClass(card) === "In review" || cardClass(card) === "Ready to merge") return "In review";
  return "Not started";
}

const ICONS: Record<StatusGroup, StatusState> = {
  "In progress": "inProgress",
  "In review": "inReview",
  "Not started": "notStarted",
  Done: "done",
};

/** The status icon of a card: a held card asks for a person, otherwise the icon of its group. */
export function iconOf(
  card: Pick<Card, "column" | "column_class" | "closed" | "held">,
): StatusState {
  if (card.held && !card.closed) return "needsYou";
  return ICONS[groupOf(card)];
}

/** One row of the list: the card with what the row says of it, already in words. */
export interface MyIssue {
  card: Card;
  key: string;
  status: StatusState;
  reason: string | undefined;
  age: string;
}

/** An age the short way the rows say it: `now`, `5m`, `3h`, `2d`; `now` and `then` are Unix seconds. */
export function shortAge(now: number, then: number): string {
  const seconds = Math.max(0, now - then);
  if (seconds < 60) return "now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m`;
  if (seconds < 86_400) return `${Math.floor(seconds / 3600)}h`;
  return `${Math.floor(seconds / 86_400)}d`;
}

/** The list items for `cards`: a group row for each status that has a card, then its cards in the order given. */
export function myIssueItems(cards: readonly Card[], now: number): ListItem<MyIssue>[] {
  const items: ListItem<MyIssue>[] = [];
  for (const name of STATUS_GROUPS) {
    const inGroup = cards.filter((card) => groupOf(card) === name);
    if (inGroup.length === 0) continue;
    items.push({ type: "group", key: `group-${name}`, name });
    for (const card of inGroup) {
      items.push({
        type: "row",
        key: String(card.id),
        data: {
          card,
          key: cardKey(card),
          status: iconOf(card),
          reason: card.held && !card.closed ? (card.held.reason ?? undefined) : undefined,
          age: shortAge(now, card.created_at),
        },
      });
    }
  }
  return items;
}
