/**
 * One row of the sidebar: an icon, a name and, at the end, the words that belong to it — a count that waits or work
 * that is running. Every row is the height of the standard's item row (36px); the chosen row is the one quiet fill
 * `--nav-selected`, with a hairline edge in the light theme, in medium weight. `Sidebar` composes the rows.
 */

import type { Icon } from "@phosphor-icons/react";
import { Link } from "react-router";
import { StatusDot } from "../../ui/components/StatusDot";
import { cn } from "../../ui/lib/cn";

/** The words after a row's name: a grey count, or work running, in green beside a green dot. */
export interface Trailing {
  text: string;
  tone: "count" | "running";
}

/** The classes every row of the sidebar shares: its height, its radius and the edge a chosen row shows. */
export const ROW =
  "flex h-(--row-list) items-center gap-2 rounded-control border border-transparent px-3 text-base";

/** The fill of a chosen row, and the text of one that waits. */
export function rowTone(current: boolean): string {
  return current
    ? "border-hairline-strong bg-nav-selected font-medium text-text in-data-[theme=dark]:border-transparent"
    : "text-sidebar-text hover:bg-row-hover hover:text-text";
}

export function Entry({
  to,
  icon: IconComponent,
  label,
  trailing,
  current,
}: {
  to: string;
  icon: Icon;
  label: string;
  trailing?: Trailing;
  /** Whether this entry leads to what is on screen now. */
  current: boolean;
}) {
  return (
    <li>
      <Link
        to={to}
        aria-current={current ? "page" : undefined}
        className={cn(ROW, rowTone(current))}
      >
        <IconComponent className="size-5 shrink-0" weight="regular" />
        <span className="min-w-0 flex-1 truncate" title={label}>
          {label}
        </span>
        {trailing?.tone === "count" && (
          <span className="shrink-0 text-sm text-secondary tabular-nums">{trailing.text}</span>
        )}
        {trailing?.tone === "running" && (
          <span className="flex shrink-0 items-center gap-1.5 text-sm text-progress-text tabular-nums">
            <StatusDot color="var(--progress)" />
            {trailing.text}
          </span>
        )}
      </Link>
    </li>
  );
}
