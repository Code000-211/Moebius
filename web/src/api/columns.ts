/**
 * The columns of a project: the six built-in ones and the custom ones its `kanban.toml` adds, each of which counts as
 * one of the built-in ones (its class). A custom column looks and behaves like its class everywhere in the app, so
 * every question the app asks about a column ("is it done?", "which icon?") is asked of the class, and the names
 * only decide where a column stands and what it says.
 *
 * The server sends the list with each board and project (`columns`); an older server sends none, which is the six
 * built-in columns.
 */
import type { Card, ColumnDef } from "./types/board";

/** The built-in columns, left to right. */
export const BUILT_IN = [
  "Backlog",
  "Not started",
  "In progress",
  "In review",
  "Ready to merge",
  "Done",
] as const;

export type ColumnClass = (typeof BUILT_IN)[number];

/** The six built-in columns as a project with no custom column has them. */
export const BUILT_IN_COLUMNS: readonly ColumnDef[] = BUILT_IN.map((name) => ({
  name,
  counts_as: name,
}));

/** Anything that carries a project's columns: a board, a project, or the list itself. */
export type HasColumns = { columns?: readonly ColumnDef[] | null } | readonly ColumnDef[] | null;

/** The columns of a board or project, left to right; the built-in six when the server named none. */
export function columnsOf(source?: HasColumns): readonly ColumnDef[] {
  const list = Array.isArray(source)
    ? (source as readonly ColumnDef[])
    : (source as { columns?: readonly ColumnDef[] | null } | null | undefined)?.columns;
  return list && list.length > 0 ? list : BUILT_IN_COLUMNS;
}

/** The names of the columns, left to right. */
export function columnNames(source?: HasColumns): string[] {
  return columnsOf(source).map((column) => column.name);
}

const isBuiltIn = (name: string): name is ColumnClass =>
  (BUILT_IN as readonly string[]).includes(name);

/**
 * The built-in column `name` counts as. A built-in name is its own class; a name the project does not list (a column
 * of a workflow of a person's own) counts as "Not started", the quiet one.
 */
export function classOf(source: HasColumns | undefined, name: string): ColumnClass {
  const found = columnsOf(source).find((column) => column.name === name)?.counts_as ?? name;
  return isBuiltIn(found) ? found : "Not started";
}

/** The class of a card's column: the card's own `column_class` when the server sent it, else worked out from the columns. */
export function cardClass(
  card: Pick<Card, "column" | "column_class">,
  source?: HasColumns,
): ColumnClass {
  if (card.column_class !== undefined && isBuiltIn(card.column_class)) return card.column_class;
  return classOf(source, card.column);
}

/** Whether the card's column counts as Done. */
export function isDone(card: Pick<Card, "column" | "column_class">, source?: HasColumns): boolean {
  return cardClass(card, source) === "Done";
}

/**
 * Gives each card that has no `column_class` (the lists and views of the server carry none; only the snapshot does) the
 * class of its column in its own project's columns. A project that is not known has the built-in columns, so its
 * custom column falls back to "Not started"; a `column_class` the server sent stays.
 */
export function withProjectClasses<T extends Pick<Card, "column" | "column_class" | "project_id">>(
  cards: readonly T[],
  projects: ReadonlyArray<{ id: number; columns?: readonly ColumnDef[] | null }>,
): T[] {
  const byId = new Map(projects.map((project) => [project.id, project]));
  return cards.map((card) =>
    card.column_class !== undefined
      ? card
      : { ...card, column_class: classOf(byId.get(card.project_id), card.column) },
  );
}

/** Where `name` stands among the columns, from 0; the number of columns for a name that is not one of them. */
export function columnIndex(source: HasColumns | undefined, name: string): number {
  const columns = columnsOf(source);
  const i = columns.findIndex((column) => column.name === name);
  return i === -1 ? columns.length : i;
}

/**
 * The columns of several projects as one list, for a page that spans projects. The built-in columns keep their order;
 * each column a project has that is not in the list yet goes right after the column that precedes it in its own
 * project, so a custom column stands after the same column as at home, and two custom columns of one project keep
 * their order. Projects are taken in the order given.
 */
export function unionColumns(lists: ReadonlyArray<readonly ColumnDef[]>): ColumnDef[] {
  const merged: ColumnDef[] = [...BUILT_IN_COLUMNS];
  for (const list of lists) {
    let after = -1;
    for (const column of list) {
      const at = merged.findIndex((each) => each.name === column.name);
      if (at !== -1) {
        after = at;
        continue;
      }
      merged.splice(after + 1, 0, column);
      after += 1;
    }
  }
  return merged;
}

/**
 * The columns a list of cards from any projects stand in, when only the cards are known: the built-in ones, and each
 * other column met (by name, so the order does not depend on the cards' order) after the last column of its class (the
 * card's `column_class`), or last when the card has none.
 */
export function columnsOfCards(
  cards: ReadonlyArray<Pick<Card, "column" | "column_class">>,
): ColumnDef[] {
  const merged: ColumnDef[] = [...BUILT_IN_COLUMNS];
  const names = [...new Set(cards.map((card) => card.column))].sort();
  for (const name of names) {
    if (merged.some((each) => each.name === name)) continue;
    const counts_as = cards.find((card) => card.column === name)?.column_class;
    let at = merged.length;
    if (counts_as !== undefined) {
      const last = merged.findLastIndex((each) => each.counts_as === counts_as);
      if (last !== -1) at = last + 1;
    }
    merged.splice(at, 0, { name, counts_as: counts_as ?? "Not started" });
  }
  return merged;
}

/**
 * Makes a board read alone: the columns of every project with cards on it as one list when the server named none for the
 * board itself (`unionColumns`), and the class of each card that has none, from the columns.
 */
export function withClasses<
  B extends {
    cards: Card[];
    columns?: ColumnDef[];
    project_columns?: Record<string, ColumnDef[]>;
  },
>(board: B): B {
  const projects = Object.values(board.project_columns ?? {});
  const columns = board.columns ?? (projects.length > 0 ? unionColumns(projects) : undefined);
  if (columns === board.columns && board.cards.every((card) => card.column_class !== undefined)) {
    return board;
  }
  return {
    ...board,
    columns,
    cards: board.cards.map((card) =>
      card.column_class === undefined
        ? { ...card, column_class: classOf(columns, card.column) }
        : card,
    ),
  };
}
