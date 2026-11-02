// From BloopAI/vibe-kanban packages/ui/src/components/ChatAggregatedToolEntries.tsx (Apache-2.0), changed: the header
// is a real button 40px tall; the count is a figure that does not shift as it grows; a file is named by its name with
// its whole path kept as the tooltip on one line whose end stays in view; the state of the group is a word with a hue;
// the caret that says the row opens is always drawn rather than shown only while the pointer is over it.
import { CaretDownIcon, ListMagnifyingGlassIcon } from "@phosphor-icons/react";
import type { ElementType } from "react";
import { cn } from "../lib/cn";
import { plural } from "../lib/format";
import { FadedLine } from "../page/FadedLine";
import { ToolStatusDot, type ToolStatusLike } from "./ToolStatusDot";

export interface AggregatedEntry {
  summary: string;
  /** The whole path behind a summary that names only a file, kept for the tooltip. */
  detail?: string;
  status?: ToolStatusLike;
  expansionKey: string;
}

interface ChatAggregatedToolEntriesProps {
  entries: AggregatedEntry[];
  expanded: boolean;
  onToggle: () => void;
  /** Label to show before the count (e.g., "Read", "Search") */
  label: string;
  /** Unit label for counting (e.g., "file", "URL") - will be pluralized automatically */
  unit: string;
  icon?: ElementType;
  className?: string;
}

// Priority: failed > denied > timed_out > pending_approval > created > success
const STATUS_PRIORITY: Record<string, number> = {
  failed: 6,
  denied: 5,
  timed_out: 4,
  pending_approval: 3,
  created: 2,
  success: 1,
};

/** The worst status among the entries, for the indicator of the whole group. */
export function worstStatus(entries: AggregatedEntry[]): ToolStatusLike | undefined {
  let worst: ToolStatusLike | undefined;
  for (const entry of entries) {
    if (!entry.status) continue;
    if (
      !worst ||
      (STATUS_PRIORITY[entry.status.status] ?? 0) > (STATUS_PRIORITY[worst.status] ?? 0)
    ) {
      worst = entry.status;
    }
  }
  return worst;
}

export function ChatAggregatedToolEntries({
  entries,
  expanded,
  onToggle,
  label,
  unit,
  icon: Icon = ListMagnifyingGlassIcon,
  className,
}: ChatAggregatedToolEntriesProps) {
  if (entries.length === 0) return null;

  // If only one entry, don't aggregate
  const only = entries.length === 1 ? entries[0] : undefined;
  if (only) {
    return (
      <div
        className={cn("flex min-h-8 flex-wrap items-center gap-3 px-3 text-left", className)}
        title={only.detail ?? only.summary}
      >
        <Icon aria-hidden="true" className="size-4 shrink-0 text-secondary" />
        <FadedLine text={only.summary} className="flex-1 font-mono text-sm text-text" />
        {only.status && <ToolStatusDot status={only.status} />}
      </div>
    );
  }

  const aggregateStatus = worstStatus(entries);

  return (
    <div className={cn("flex flex-col", className)}>
      <button
        type="button"
        className="flex min-h-8 w-full cursor-pointer items-center gap-3 rounded-control px-3 text-left transition-colors duration-(--motion-routine) ease-out hover:bg-hover"
        onClick={onToggle}
        aria-expanded={expanded}
      >
        <Icon aria-hidden="true" className="size-4 shrink-0 text-secondary" />
        <span className="min-w-0 flex-1 break-words text-base text-text">
          {label} · <span className="tabular-nums">{plural(entries.length, unit, `${unit}s`)}</span>
        </span>
        {aggregateStatus && <ToolStatusDot status={aggregateStatus} />}
        <CaretDownIcon
          aria-hidden="true"
          className={cn(
            "size-4 shrink-0 text-secondary transition-transform duration-(--motion-routine) ease-out",
            !expanded && "-rotate-90",
          )}
        />
      </button>

      {expanded && (
        <div className="flex flex-col gap-1 pl-4">
          {entries.map((entry) => (
            <div
              key={entry.expansionKey}
              title={entry.detail ?? entry.summary}
              className="flex min-h-8 flex-wrap items-center gap-3 px-3"
            >
              <Icon aria-hidden="true" className="size-4 shrink-0 text-secondary" />
              <FadedLine text={entry.summary} className="flex-1 font-mono text-sm text-text" />
              {entry.status && <ToolStatusDot status={entry.status} />}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
