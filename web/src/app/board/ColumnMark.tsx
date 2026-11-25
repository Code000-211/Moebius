/**
 * The mark in a column's title: the status icon of the state it holds, so the board reads as states at a glance.
 *
 * A column has the icon of its class: a custom column looks like the built-in one it counts as. A name that is none of
 * them (a workflow of a person's own) gets the dashed ring of "not started", which says "a column" and nothing more.
 * The word beside the mark is what carries the meaning, so the icon is decorative.
 */
import { classOf, type HasColumns } from "../../api/columns";
import { StatusIcon, type StatusState } from "../../ui/page/StatusIcon";

const STATES: Record<string, StatusState> = {
  Backlog: "notStarted",
  "Not started": "notStarted",
  "In progress": "running",
  "In review": "inReview",
  "Ready to merge": "inReview",
  Done: "done",
};

/** `columns` are the project's, when known; `counts` is the class the server named for the column, which wins. */
export function ColumnMark({
  name,
  counts,
  columns,
}: {
  name: string;
  counts?: string;
  columns?: HasColumns;
}) {
  const state = STATES[counts ?? ""] ?? STATES[classOf(columns, name)] ?? "notStarted";
  return <StatusIcon state={state} decorative />;
}
