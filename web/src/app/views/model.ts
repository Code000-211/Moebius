/**
 * What the Views page and the board's "Save view" know about a view: how the board's filters become a view's filter,
 * how a view's cards are set out in each layout (list, board, calendar by due date, timeline by cycle), and the words
 * that describe a view's filter. The server keeps the views and works out the cards for each person.
 */
import type { Cycle } from "../../api/types/cycles";
import type { View, ViewCard, ViewFields, ViewFilter, ViewLayout } from "../../api/types/views";
import { COLUMNS, type Filters } from "../board/model";
import { boardPath } from "../pages";

export const LAYOUTS: Array<[ViewLayout, string]> = [
  ["list", "List"],
  ["board", "Board"],
  ["calendar", "Calendar"],
  ["timeline", "Timeline"],
];

export const SORTS: Array<[string, string]> = [
  ["created", "Order made"],
  ["updated", "Last moved"],
  ["priority", "Priority"],
  ["due", "Due date"],
  ["title", "Title"],
  ["estimate", "Estimate"],
];

export const NO_FILTER: ViewFilter = {
  projects: [],
  repositories: [],
  labels: [],
  assignees: [],
  priorities: [],
  cycles: [],
  milestones: [],
  columns: [],
  authors: [],
  agents: [],
  statuses: [],
  due: "",
  text: "",
};

/** How the board's sorts are kept in a view: its sort and whether it is turned round. */
const SORT_OF_BOARD: Record<string, [string, boolean]> = {
  "": ["created", false],
  newest: ["created", true],
  title: ["title", false],
  priority: ["priority", false],
  due: ["due", false],
};

/**
 * The view that "Save view" makes of a board: the repository it shows, its column, agent, status, label, priority and
 * due filters, and its layout and sort. The agent is kept as the agents that ran on a card (any run, where the board's
 * filter looks at the latest), and the status and the due range as the server works them out each time the view is
 * opened.
 */
export function viewOfBoard(
  name: string,
  repository: number,
  filters: Filters,
  layout: string,
  sort: string,
): ViewFields {
  const [by, descending] = SORT_OF_BOARD[sort] ?? ["created", false];
  const one = (value: string) => (value === "" ? [] : [value]);
  return {
    name,
    shared: false,
    layout: layout === "list" ? "list" : "board",
    sort: by,
    descending,
    filter: {
      repositories: [repository],
      columns: one(filters.column),
      agents: one(filters.agent),
      statuses: one(filters.status),
      labels: one(filters.label),
      priorities: one(filters.priority),
      due: filters.due,
    },
  };
}

/**
 * The filters the board can set for `view`, or none when the view asks for more than the board's filter bar holds:
 * one repository and at most one of each of a column, agent, status, label and priority, and nothing else.
 */
export function boardFiltersOf(view: View): { repository: number; filters: Filters } | null {
  const f = view.filter;
  const single = (values: readonly string[]) => (values.length <= 1 ? (values[0] ?? "") : null);
  const [column, agent, status, label, priority] = [
    f.columns,
    f.agents,
    f.statuses,
    f.labels,
    f.priorities,
  ].map(single);
  const repository = f.repositories[0];
  if (
    f.repositories.length !== 1 ||
    repository === undefined ||
    [column, agent, status, label, priority].includes(null) ||
    f.projects.length + f.assignees.length + f.cycles.length + f.milestones.length > 0 ||
    f.authors.length > 0 ||
    f.text !== ""
  ) {
    return null;
  }
  return {
    repository,
    filters: {
      column: column ?? "",
      agent: agent ?? "",
      status: status ?? "",
      label: label ?? "",
      priority: priority ?? "",
      due: f.due,
    },
  };
}

/**
 * Where a saved view opens: the board of its repository with the view's filters, layout and sort, and `view` naming it
 * in the filter bar. Only a view of the `board` or `list` layout (the two layouts of the board) whose filter the board
 * can set opens there; any other opens on the Views page (null).
 */
export function boardOfView(view: View): string | null {
  const board = view.layout === "board" || view.layout === "list" ? boardFiltersOf(view) : null;
  if (board === null) return null;
  const query = new URLSearchParams();
  if (view.layout === "list") query.set("layout", "list");
  for (const [key, value] of Object.entries(board.filters)) if (value !== "") query.set(key, value);
  const sort = Object.entries(SORT_OF_BOARD).find(
    ([, [by, descending]]) => by === view.sort && descending === view.descending,
  );
  if (sort && sort[0] !== "") query.set("sort", sort[0]);
  query.set("view", String(view.id));
  return `${boardPath(board.repository)}?${query.toString()}`;
}

/** The fields of an existing view, to change some of them. */
export function fieldsOf(view: View): ViewFields {
  return {
    name: view.name,
    shared: view.shared,
    filter: view.filter,
    layout: view.layout,
    sort: view.sort,
    descending: view.descending,
  };
}

/** What a view's filter asks, in words: "label bug · priority urgent, high"; "everything" when it asks nothing. */
export function filterWords(filter: ViewFilter): string {
  const parts: Array<[string, readonly (string | number)[]]> = [
    ["project", filter.projects],
    ["repository", filter.repositories],
    ["label", filter.labels],
    ["assignee", filter.assignees],
    ["priority", filter.priorities],
    ["cycle", filter.cycles],
    ["milestone", filter.milestones],
    ["column", filter.columns],
    ["author", filter.authors],
    ["agent", filter.agents],
    ["status", filter.statuses],
    ["due", filter.due === "" ? [] : [filter.due]],
    ["text", filter.text === "" ? [] : [`"${filter.text}"`]],
  ];
  const words = parts
    .filter(([, values]) => values.length > 0)
    .map(([n, v]) => `${n} ${v.join(", ")}`);
  return words.length === 0 ? "everything" : words.join(" · ");
}

/** A heading of a layout and the cards under it. */
export interface Group {
  key: string;
  name: string;
  cards: ViewCard[];
}

function grouped(
  cards: readonly ViewCard[],
  keyOf: (card: ViewCard) => string,
  nameOf: (key: string, card: ViewCard) => string,
  order: (a: string, b: string) => number,
  last: string,
): Group[] {
  const groups = new Map<string, Group>();
  for (const card of cards) {
    const key = keyOf(card);
    const group = groups.get(key) ?? { key, name: nameOf(key, card), cards: [] };
    group.cards.push(card);
    groups.set(key, group);
  }
  return [...groups.values()].sort((a, b) =>
    a.key === last ? 1 : b.key === last ? -1 : order(a.key, b.key),
  );
}

/** The board layout: a lane for each column, the usual ones first, then any other in the order met. */
export function byColumn(cards: readonly ViewCard[]): Group[] {
  const rank = (column: string) => {
    const i = (COLUMNS as readonly string[]).indexOf(column);
    return i === -1 ? COLUMNS.length : i;
  };
  return grouped(
    cards,
    (c) => c.column,
    (key) => key,
    (a, b) => rank(a) - rank(b),
    "\u0000",
  );
}

/** The calendar layout: a heading for each due day, the earliest first, the cards without a date last. */
export function byDueDate(cards: readonly ViewCard[]): Group[] {
  return grouped(
    cards,
    (c) => c.due ?? "",
    (key) => (key === "" ? "No due date" : key),
    (a, b) => a.localeCompare(b),
    "",
  );
}

/** The timeline layout: a heading for each cycle, the earliest start first, the cards in no cycle last. */
export function byCycle(cards: readonly ViewCard[]): Group[] {
  const cycles = new Map<string, Cycle>();
  for (const card of cards) if (card.cycle) cycles.set(String(card.cycle.id), card.cycle);
  const start = (key: string) => cycles.get(key)?.starts_on ?? "";
  return grouped(
    cards,
    (c) => (c.cycle ? String(c.cycle.id) : ""),
    (key, card) =>
      key === ""
        ? "No cycle"
        : `${card.cycle?.name} · ${card.cycle?.starts_on} to ${card.cycle?.ends_on}`,
    (a, b) => start(a).localeCompare(start(b)),
    "",
  );
}

/** The headings of `layout`: one heading for the list, which has none. */
export function groupsOf(layout: ViewLayout, cards: readonly ViewCard[]): Group[] {
  if (layout === "board") return byColumn(cards);
  if (layout === "calendar") return byDueDate(cards);
  if (layout === "timeline") return byCycle(cards);
  return [{ key: "", name: "", cards: [...cards] }];
}
