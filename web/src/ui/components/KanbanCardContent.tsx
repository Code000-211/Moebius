// From BloopAI/vibe-kanban packages/ui/src/components/KanbanCardContent.tsx (Apache-2.0), changed: no i18n; the card is
// flat and quiet, and shows only its status icon, its number in mono, its title (which wraps, however long) and the
// avatar of the person it is given to, with one pale line under them while an agent works on it or it needs a person —
// in place of priority, tags, relationships, the description preview, state words, round bars and links; the "more
// actions" button is a slot (`menu`) the page fills with its own menu, beside the avatar; the card is opened by its
// title being a button, so the card itself stays a group; a slot for the stack of its sub-issues (`stack`), and a slot
// for the box that chooses the card (`select`), which stands in the card's top left corner. A card that is someone
// else's work (taken on another server, or a pull request someone else opened) wears that person's avatar, told as
// such (`assigneesLabel`).

import type { ReactNode } from "react";
import { cn } from "../lib/cn";
import { AvatarStack } from "../page/Avatar";
import { StatusIcon, type StatusState } from "../page/StatusIcon";

export type KanbanCardContentProps = {
  /** The card's number as it is written, such as `MOE-12` or `#12`. */
  displayId: string;
  title: string;
  /** The icon of the card; one of the five of the standard. */
  status: StatusState;
  /** The pale line under the title: what the agent is doing, or why the card waits (grey), or that it needs a person (orange). */
  activity?: { text: string; tone: "quiet" | "needs-you"; hint?: string } | null;
  /** The handles of the people the card is given to, as avatars; none while it is no one's. */
  assignees?: string[];
  /** What the avatars are of, for a screen reader; "Given to" unless the card is someone else's work (see `AvatarStack`). */
  assigneesLabel?: string;
  /** A card in Done is drawn quiet. */
  muted?: boolean;
  /**
   * Opens the card. With it the title is a button a keyboard and a screen reader can use; without it — on the drag
   * overlay, which only shows what is under the pointer — the title stays the text it is.
   */
  onOpen?: () => void;
  /** The card's menu, beside its number; left out on the drag overlay, which is only a picture of the card. */
  menu?: ReactNode;
  /** The box that chooses the card, drawn in its top left corner; left out where a card cannot be chosen. */
  select?: ReactNode;
  /** What a card with sub-issues shows at its foot: how many are done, and the control that opens the stack. */
  stack?: ReactNode;
  className?: string;
};

export function KanbanCardContent({
  displayId,
  title,
  status,
  activity,
  assignees = [],
  assigneesLabel,
  muted = false,
  onOpen,
  menu,
  select,
  stack,
  className,
}: KanbanCardContentProps) {
  const titleClass = cn(
    "min-w-0 text-base font-medium [overflow-wrap:anywhere]",
    muted ? "text-secondary" : "text-text",
  );
  return (
    <div className={cn("relative flex min-w-0 flex-col gap-1", className)}>
      <div className="relative flex min-h-6 min-w-0 items-center gap-2">
        {select && <div className="absolute -top-1.5 -left-3 z-10">{select}</div>}
        <StatusIcon state={status} />
        <span className="font-mono text-xs text-muted tabular-nums">{displayId}</span>
        <span className="ml-auto flex shrink-0 items-center gap-1">
          <AvatarStack handles={assignees} label={assigneesLabel} />
          {menu && (
            // Quiet until the card is pointed at, focused or its menu is open.
            <span className="-my-2 -mr-2 opacity-0 group-focus-within:opacity-100 group-hover:opacity-100 has-[[aria-expanded=true]]:opacity-100">
              {menu}
            </span>
          )}
        </span>
      </div>
      {onOpen ? (
        <button
          type="button"
          onClick={(e) => {
            // The card answers a click on itself as well; this one is the title's, and opens the card once.
            e.stopPropagation();
            onOpen();
          }}
          className={cn("appearance-none bg-transparent p-0 text-left", titleClass)}
        >
          {title}
        </button>
      ) : (
        <span className={titleClass}>{title}</span>
      )}
      {activity && (
        <span
          data-testid="card-activity"
          title={activity.hint}
          className={cn(
            "text-sm [overflow-wrap:anywhere]",
            activity.tone === "needs-you" ? "text-needs-you-text" : "text-muted",
          )}
        >
          {activity.text}
        </span>
      )}
      {stack}
    </div>
  );
}
