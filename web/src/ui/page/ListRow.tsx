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
 *   action={<Button variant="quiet">Approve</Button>}
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

/** The narrowest id column, in characters: `MOE-123` fits. */
const ID_MIN = 8;
/** The widest, so one long project key cannot take the title's room. */
const ID_MAX = 16;

/** How many characters the id column of a list needs: its longest id, between the narrowest and the widest. */
export function idChars(ids: readonly string[]): number {
  const longest = ids.reduce((most, id) => Math.max(most, id.length), 0);
  return Math.min(ID_MAX, Math.max(ID_MIN, longest));
}

/** An id cut to `chars`: when too long, the project part is cut with an ellipsis and the number stays whole. */
export function fitId(id: string, chars: number): string {
  if (id.length <= chars) return id;
  const dash = id.lastIndexOf("-");
  if (dash <= 0) return id;
  const number = id.slice(dash);
  const room = chars - number.length - 1;
  return room >= 1 ? `${id.slice(0, room)}…${number}` : `…${number}`;
}

function position(at: RowPosition | undefined) {
  return at ? { "aria-posinset": at.index, "aria-setsize": at.size } : {};
}

export function ListRow({
  status = "notStarted",
  mark,
  id,
  idChars: chars = ID_MIN,
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
  /** The issue's id, drawn in mono; a row with none (a Retro proposal) leaves no gap for it. */
  id: string;
  /** The width of the id column in characters, from `idChars` of the list; keeps a long key's number readable. */
  idChars?: number;
  title: string;
  /** The one pale reason or activity ("Waiting for approval", "editing engine.rs"). */
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
  /** What a page's tests look a row up by, where it names its own. */
  "data-testid"?: string;
  "data-kind"?: string;
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
      {id !== "" && (
        <span
          className="shrink-0 truncate font-mono text-xs text-muted"
          style={{ width: `${chars}ch` }}
          title={id}
        >
          {fitId(id, chars)}
        </span>
      )}
      {/* The title and the reason are a line that wraps and is clipped: a reason that does not fit after the title
          goes away whole, instead of shrinking to a letter. */}
      <div className="flex h-9 min-w-0 flex-1 flex-wrap gap-x-2.5 overflow-hidden">
        <span className="max-w-full truncate font-medium leading-9 text-text">{title}</span>
        {reason && (
          <span className="max-w-full truncate whitespace-nowrap leading-9 text-muted">
            {reason}
          </span>
        )}
      </div>
      <span className="flex shrink-0 items-center gap-2.5">
        {action}
        {owner && <Avatar handle={owner} className="size-[18px]" />}
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
