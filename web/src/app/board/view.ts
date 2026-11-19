/**
 * What the board offers the shared filter bar (`ui/filters`): the fields it can be filtered by and their values, what
 * Display can group, sort and show, and the cards of the list layout, which draws every card that passes the filters
 * in rows under group headings rather than in lanes. The address keeps the filters under the keys `column`, `agent`,
 * `status` and `label`, as it did when they were selects.
 */
import type { Board, Card, InboxItem } from "../../api/types/board";
import type { DisplayChoices, FilterField } from "../../ui/filters/model";
import {
  agentsOf,
  boardColumns,
  cardStatus,
  type Filters,
  inboxOf,
  labelsOf,
  matchesCard,
  orderCards,
  STATUS_LABELS,
} from "./model";

/** The keys of the board's filters in the address, in the order the bar offers them. */
export const FILTER_KEYS = ["column", "agent", "status", "label"] as const;

/** The query parameters a saved view of the board holds: its filters and its Display choices. */
export const VIEW_KEYS = [...FILTER_KEYS, "layout", "group", "sort", "hide"];

/** The fields the board can be filtered by. Label is offered while the issues carry any, or one is set. */
export function filterFields(board: Board, filters: Filters): FilterField[] {
  const pairs = (names: string[]): Array<[string, string]> => names.map((n) => [n, n]);
  const labels = labelsOf(board);
  if (filters.label !== "" && !labels.includes(filters.label)) labels.push(filters.label);
  return [
    { key: "column", label: "Column", options: pairs(boardColumns(board)) },
    { key: "agent", label: "Agent", options: pairs(agentsOf(board)) },
    { key: "status", label: "Status", options: Object.entries(STATUS_LABELS) },
    { key: "label", label: "Label", options: pairs(labels) },
  ];
}

/** How the list layout groups its cards; the empty value is the column, as on the board. */
export const GROUPS: Array<[string, string]> = [
  ["", "Column"],
  ["status", "Status"],
  ["agent", "Agent"],
  ["label", "Label"],
  ["none", "None"],
];

/** How cards are ordered; the empty value is by number, with Done newest first. */
export const SORTS: Array<[string, string]> = [
  ["", "Number"],
  ["newest", "Newest first"],
  ["title", "Title"],
];

/** The parts of a card that can be switched off. */
export const CARD_FIELDS: Array<[string, string]> = [
  ["labels", "Labels"],
  ["description", "Description"],
  ["detail", "Agent and round"],
  ["rounds", "Round limit"],
  ["pullRequest", "Pull request"],
  ["claim", "Claimed by"],
];

export const DISPLAY_CHOICES: DisplayChoices = {
  groups: GROUPS,
  sorts: SORTS,
  fields: CARD_FIELDS,
};

/** One heading of the list layout and the cards under it. */
export interface ListGroup {
  key: string;
  name: string;
  cards: Card[];
}

/**
 * The cards of the list layout: every card that passes the filters (a sub-issue is a row like any other), grouped as
 * asked and ordered as asked, groups with no card left out. A card with several labels is under each of them.
 */
export function listGroups(
  board: Board,
  inbox: readonly InboxItem[],
  filters: Filters,
  group: string,
  sort: string,
): ListGroup[] {
  const passing = board.cards.filter(
    (card) =>
      (filters.column === "" || card.column === filters.column) &&
      matchesCard(card, filters, inbox, board),
  );
  const make = (key: string, name: string, of: (card: Card) => boolean): ListGroup => ({
    key,
    name,
    cards: orderCards(passing.filter(of), sort, group === "" ? key : undefined),
  });
  let groups: ListGroup[];
  switch (group) {
    case "status":
      groups = Object.entries(STATUS_LABELS).map(([key, name]) =>
        make(key, name, (card) => cardStatus(card, inboxOf(inbox, card), board) === key),
      );
      break;
    case "agent":
      groups = [
        ...agentsOf(board).map((name) => make(name, name, (card) => card.run?.agent === name)),
        make("", "No agent", (card) => !card.run),
      ];
      break;
    case "label":
      groups = [
        ...labelsOf(board).map((name) => make(name, name, (card) => card.labels.includes(name))),
        make("", "No label", (card) => card.labels.length === 0),
      ];
      break;
    case "none":
      groups = [make("", "Cards", () => true)];
      break;
    default:
      groups = boardColumns(board).map((name) => make(name, name, (card) => card.column === name));
  }
  return groups.filter((each) => each.cards.length > 0);
}
