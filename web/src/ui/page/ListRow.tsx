/**
 * The row of every list of the app (Inbox, My issues, a project's list, Fleet) and the row that names a group in it.
 *
 * A row is 36px (`h-9`) with a hairline under it and no shadow: the status icon, the id in mono, the full title on one
 * line cut with an ellipsis, one pale reason or activity, then pushed to the right an optional action, the owner's
 * avatar (only where a person owns it) and the age in a column of fixed width. The selected row has a 2px indigo edge
 * on its left and the accent tint; a row under the pointer has the hover fill. The edge is drawn by a pseudo element,
 * so no box shadow is involved. `VirtualList` lays rows out and sets their position in the list; used alone, a row
 * needs none.
 *
 * The row carries the `group/row` class, so a part of it can appear only on the selected or hovered row (Fleet's
 * "Stop" and cost) with `invisible group-hover/row:visible` or the like, without the row changing height.
 *
 * ```tsx
 * <ListRow
 *   status="needsYou"
 *   id="MOE-209"
 *   title="Wiki save overwrites a newer version"
 *   reason="Review found a data loss"
 *   action={<Button variant="quiet">Approve plan</Button>}
 *   owner="leon"
 *   age="2m"
 *   selected
 * />
 * <GroupRow name="Needs you" />
 * ```
 */
import type { MouseEvent, ReactNode } from "react";
import { cn } from "../lib/cn";
import { Avatar } from "./Avatar";
import { StatusIcon, type StatusState } from "./StatusIcon";

/** Where a row stands in the whole list, one-based, counting group rows too; the list sets it. */
export interface RowPosition {
  index: number;
  size: number;
}

function position(at: RowPosition | undefined) {
  return at ? { "aria-posinset": at.index, "aria-setsize": at.size } : {};
}

export function ListRow({
  status = "notStarted",
  mark,
  id,
  title,
  reason,
  action,
  owner,
  age,
  selected = false,
  at,
  onClick,
  className,
  ...data
}: {
  /** The state the status icon draws; a row with a `mark` of its own needs none. */
  status?: StatusState;
  /** What stands in the place of the status icon, for a row whose state is not one of the five (a run's outcome). */
  mark?: ReactNode;
  /** The issue's id, drawn in mono. */
  id: string;
  title: string;
  /** The one pale reason or activity ("Plan ready", "editing engine.rs"). */
  reason?: ReactNode;
  /** The one action the row needs, on the row. */
  action?: ReactNode;
  /** The handle of the person who owns it; none for a run of an agent. */
  owner?: string;
  /** The age, already said the compact way (`2m`). */
  age?: string;
  selected?: boolean;
  at?: RowPosition;
  onClick?: (event: MouseEvent<HTMLDivElement>) => void;
  className?: string;
  "data-key"?: string;
}) {
  return (
    // biome-ignore lint/a11y/useKeyWithClickEvents: the list owns the keys (arrows and Enter); a click is the pointer's way to the same open
    // biome-ignore lint/a11y/useSemanticElements: the rows are items of a virtual list whose wrappers are not a <ul>, so the role is stated
    <div
      role="listitem"
      data-testid="list-row"
      data-selected={selected ? "true" : undefined}
      aria-current={selected ? "true" : undefined}
      onClick={onClick}
      {...position(at)}
      {...data}
      className={cn(
        "group/row relative flex h-9 w-full items-center gap-2.5 border-b border-hairline px-4 text-base",
        onClick && "cursor-pointer",
        selected
          ? "bg-accent-tint before:absolute before:inset-y-0 before:left-0 before:w-0.5 before:bg-accent"
          : "hover:bg-row-hover",
        className,
      )}
    >
      {mark ?? <StatusIcon state={status} />}
      <span className="w-16 shrink-0 truncate font-mono text-xs text-muted">{id}</span>
      <span className="min-w-0 shrink truncate font-medium text-text">{title}</span>
      {reason && <span className="min-w-0 shrink-[2] truncate text-muted">{reason}</span>}
      <span className="ml-auto flex shrink-0 items-center gap-2.5">
        {action}
        {owner && <Avatar handle={owner} />}
        {age !== undefined && (
          <span className="w-10 text-right text-xs tabular-nums text-muted">{age}</span>
        )}
      </span>
    </div>
  );
}

/** The row that names a group: its name and nothing else, 30px, on the group fill. */
export function GroupRow({
  name,
  at,
  className,
}: {
  name: string;
  at?: RowPosition;
  className?: string;
}) {
  return (
    // biome-ignore lint/a11y/useSemanticElements: the rows are items of a virtual list whose wrappers are not a <ul>, so the role is stated
    <div
      role="listitem"
      data-testid="group-row"
      {...position(at)}
      className={cn(
        "flex h-[30px] w-full items-center border-b border-hairline bg-group-fill px-4 text-xs font-medium text-secondary",
        className,
      )}
    >
      {name}
    </div>
  );
}
