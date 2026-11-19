/**
 * The mark in a column's title: the status icon of the state it holds, so the board reads as states at a glance.
 *
 * A workflow is a person's own file, so any name can appear; the names of the default workflow get an icon of their
 * own and everything else gets the quiet ring of "not started", which says "a column" and nothing more. The word beside
 * the mark is what carries the meaning, so the icon is decorative.
 */
import { StatusIcon, type StatusState } from "../../ui/page/StatusIcon";

const STATES: Record<string, StatusState> = {
  Backlog: "backlog",
  "Not started": "notStarted",
  "In progress": "inProgress",
  "In review": "inReview",
  "Ready to merge": "readyToMerge",
  Done: "done",
};

export function ColumnMark({ name }: { name: string }) {
  return <StatusIcon state={STATES[name] ?? "notStarted"} decorative />;
}
