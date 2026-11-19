/**
 * One row of the sidebar's list, and the words and marks that stand at its end: a count that waits, work that is
 * running, or the state of what the row leads to. The rail draws the same row as the icon alone, with those words
 * carried by the link's name and its tooltip. `Sidebar` composes the rows; `ProjectSection` hands its Board row a mark.
 */

import type { Icon } from "@phosphor-icons/react";
import { Link } from "react-router";
import { StatusDot } from "../../ui/components/StatusDot";
import { Tooltip } from "../../ui/components/Tooltip";
import { cn } from "../../ui/lib/cn";

/** How the words after an entry's name are coloured: a count that waits, work that is running, or the plain state. */
export type TrailingTone = "attention" | "progress" | "secondary";

/** The words after an entry's name: the count or state of what it leads to. Colour supports them, never replaces them. */
export interface Trailing {
  text: string;
  tone: TrailingTone;
}

/**
 * The mark at the end of a row that stands for the state of what it leads to.
 *
 * A state almost every row is in is a mark and no word: a list of repositories that are all waiting says "Idle" as
 * many times as it has rows, and the word that repeats is the one nobody reads. The word is kept where a person
 * cannot see the colour — in the name of the row, and in the rail, where the words themselves are gone.
 */
export interface RowState {
  color: string;
  word: string;
  /** The colour of the word, when it is printed. */
  tone: TrailingTone;
  /** Whether the word stands beside the mark, as a count does, or stays quiet, as a state every row shares does. */
  printed: boolean;
}

/** The fill of each tone. */
const TRAILING: Record<TrailingTone, string> = {
  attention: "text-attention-text",
  progress: "text-progress-text",
  secondary: "text-secondary",
};

export function Entry({
  to,
  icon: IconComponent,
  mark,
  label,
  collapsed,
  trailing,
  current,
}: {
  to: string;
  icon?: Icon;
  /** The state of what the row leads to, at the end of the row where its words used to stand. */
  mark?: RowState;
  label: string;
  collapsed: boolean;
  trailing?: Trailing;
  /** Whether this entry leads to what is on screen now. */
  current: boolean;
}) {
  // The rail has no room for the words after a name, so the link takes them into its own name and the tooltip repeats
  // them: the state of a board is never only the colour of its dot.
  const quiet = mark?.word ?? trailing?.text;
  const name = collapsed && quiet ? `${label}. ${quiet}` : label;
  // A row whose state is a mark has the word nowhere it can be read as a word, so the name of the link takes it —
  // the same words the rail is given, which is also how a name stays one name whatever the shape of the sidebar.
  const spoken = collapsed || !mark ? undefined : `${label}. ${mark.word}`;
  const rows = (
    <>
      {IconComponent && <IconComponent className="size-5 shrink-0" weight="regular" />}
      {/* The rail has no end to a row to put a mark at, so there it rides beside the icon; in the list it stands
          where the words it replaces stood. */}
      {collapsed && mark && <StatusDot color={mark.color} />}
      {!collapsed && (
        <>
          <span className="min-w-0 flex-1 truncate" title={label}>
            {label}
          </span>
          {trailing && (
            <span className={cn("shrink-0 text-sm tabular-nums", TRAILING[trailing.tone])}>
              {trailing.text}
            </span>
          )}
          {mark && (
            <>
              {mark.printed && (
                <span className={cn("shrink-0 text-sm tabular-nums", TRAILING[mark.tone])}>
                  {mark.word}
                </span>
              )}
              <StatusDot color={mark.color} />
            </>
          )}
        </>
      )}
    </>
  );
  const shape = cn(
    "flex min-h-8 items-center gap-2 rounded-control px-3 text-base",
    collapsed && "justify-center px-0",
  );
  // The entry that stands is a slightly raised surface, the texture of the sidebar.
  const standing = "bg-raised font-semibold text-text shadow-card";
  const waiting = "text-sidebar-text hover:bg-hover hover:text-text";
  // Which entry stands is worked out here rather than asked of the router in a callback, because the rail wraps the
  // row in a tooltip and a tooltip's trigger folds a function `className` into text — the row would keep its link and
  // lose its whole shape, which is what made the rail's icons sit uncentred at half height.
  const link = (
    <Link
      to={to}
      aria-label={collapsed ? name : spoken}
      aria-current={current ? "page" : undefined}
      className={cn(shape, current ? standing : waiting)}
    >
      {rows}
    </Link>
  );
  return (
    <li>
      {collapsed ? (
        <Tooltip content={name} side="right">
          {link}
        </Tooltip>
      ) : (
        link
      )}
    </li>
  );
}
