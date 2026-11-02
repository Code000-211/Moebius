/**
 * The mark in a column's title: the shape and colour of the state it holds, so the board reads as states at a glance.
 *
 * A workflow is a person's own file, so any name can appear; the names of the default workflow get a mark of their
 * own and everything else gets the quiet circle, which says "a column" and nothing more. The word beside the mark is
 * what carries the meaning — colour and shape only make it quick to find.
 */
import {
  CheckCircleIcon,
  CircleDashedIcon,
  CircleHalfIcon,
  CircleIcon,
  EyeIcon,
  GitPullRequestIcon,
  type Icon,
} from "@phosphor-icons/react";

/** The mark of one column: its icon and the token its colour comes from. */
const MARKS: Record<string, { icon: Icon; colour: string }> = {
  Backlog: { icon: CircleDashedIcon, colour: "text-muted" },
  "Not started": { icon: CircleIcon, colour: "text-secondary" },
  "In progress": { icon: CircleHalfIcon, colour: "text-progress" },
  "In review": { icon: EyeIcon, colour: "text-secondary" },
  "Ready to merge": { icon: GitPullRequestIcon, colour: "text-attention-text" },
  Done: { icon: CheckCircleIcon, colour: "text-complete-text" },
};

const FALLBACK = { icon: CircleIcon, colour: "text-muted" };

export function ColumnMark({ name }: { name: string }) {
  const mark = MARKS[name] ?? FALLBACK;
  return <mark.icon className={`size-4 shrink-0 ${mark.colour}`} aria-hidden="true" />;
}
