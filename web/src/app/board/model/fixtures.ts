/**
 * Cards, boards and inbox items the tests of the board model build on.
 */
import type { Board, Card, InboxItem } from "../../../api/types/board";

export function card(id: number, extra: Partial<Card> = {}): Card {
  return {
    id,
    repository_id: 1,
    project_id: 1,
    number: id,
    title: `Card ${id}`,
    body: "",
    author: "person",
    column: "Not started",
    created_at: 0,
    note: null,
    retried_after: null,
    rounds: 0,
    tokens: 0,
    run: null,
    blocked_by: [],
    refusals: 0,
    change: null,
    parent: null,
    counts_as_sub_issue: true,
    closed: false,
    labels: [],
    claimed_by: null,
    assignees: [],
    priority: "none",
    due: null,
    estimate: null,
    ...extra,
  };
}

/** A card held by a person with a reason. */
export const held = { reason: "wait for the design", by: "person", at: 1 };

export function board(cards: Card[], extra: Partial<Board> = {}): Board {
  return {
    repository: {
      id: 1,
      name: "demo",
      path: "/r",
      forge: "github",
      default_branch: "main",
      home_project_id: 1,
      github: "acme/demo",
      trusted: [],
    },
    cards,
    round_limit: 5,
    waiting: {},
    changes: [],
    ...extra,
  };
}

export function item(
  kind: string,
  cardId: number | null,
  data: Record<string, unknown> = {},
): InboxItem {
  return {
    id: 1,
    kind,
    card_id: cardId,
    title: kind,
    data,
    resolved_at: null,
    at: 0,
    assignees: [],
  };
}

export const running = { id: 1, agent: "Coder", purpose: "code", status: "running" as const };

export const run = running;
