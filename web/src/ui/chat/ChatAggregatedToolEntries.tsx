// From BloopAI/vibe-kanban packages/ui/src/components/ChatAggregatedToolEntries.tsx (Apache-2.0), changed: the worst-status choice is a small function above the component; the header is a real button (keyboard and screen readers); the label of the count is not pluralised by appending an "s" to the unit when the caller gives a plural.
import { CaretRightIcon, ListMagnifyingGlassIcon } from "@phosphor-icons/react";
import type { ElementType } from "react";
import { cn } from "../lib/cn";
import { ToolStatusDot, type ToolStatusLike } from "./ToolStatusDot";

export interface AggregatedEntry {
  summary: string;
  status?: ToolStatusLike;
  expansionKey: string;
}

interface ChatAggregatedToolEntriesProps {
  entries: AggregatedEntry[];
  expanded: boolean;
  isHovered: boolean;
  onToggle: () => void;
  onHoverChange: (hovered: boolean) => void;
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
  isHovered,
  onToggle,
  onHoverChange,
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
      <div className={cn("flex items-center gap-base text-sm text-low", className)}>
        <span className="relative shrink-0 pt-0.5">
          <Icon className="size-icon-base" />
          {only.status && (
            <ToolStatusDot status={only.status} className="absolute -bottom-0.5 -left-0.5" />
          )}
        </span>
        <span className="truncate">{only.summary}</span>
      </div>
    );
  }

  const aggregateStatus = worstStatus(entries);

  return (
    <div className={cn("flex flex-col", className)}>
      {/* Header row - clickable to expand/collapse */}
      <button
        type="button"
        className="group flex items-center gap-base text-left text-sm text-low"
        onClick={onToggle}
        onMouseEnter={() => onHoverChange(true)}
        onMouseLeave={() => onHoverChange(false)}
        aria-expanded={expanded}
      >
        <span className="relative shrink-0 pt-0.5">
          {isHovered ? (
            <CaretRightIcon
              className={cn(
                "size-icon-base transition-transform duration-150",
                expanded && "rotate-90",
              )}
            />
          ) : (
            <Icon className="size-icon-base" />
          )}
          {aggregateStatus && (
            <ToolStatusDot status={aggregateStatus} className="absolute -bottom-0.5 -left-0.5" />
          )}
        </span>
        <span className="truncate">
          {label} · {entries.length} {entries.length === 1 ? unit : `${unit}s`}
        </span>
      </button>

      {/* Expanded content */}
      {expanded && (
        <div className="ml-6 flex flex-col gap-0.5 pt-1">
          {entries.map((entry) => (
            <div
              key={entry.expansionKey}
              className="flex items-center gap-base pl-base text-sm text-low"
            >
              <span className="relative shrink-0 pt-0.5">
                <Icon className="size-icon-base" />
                {entry.status && (
                  <ToolStatusDot status={entry.status} className="absolute -bottom-0.5 -left-0.5" />
                )}
              </span>
              <span className="truncate font-mono text-xs">{entry.summary}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
