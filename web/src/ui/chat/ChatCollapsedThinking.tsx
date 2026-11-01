// From BloopAI/vibe-kanban packages/ui/src/components/ChatCollapsedThinking.tsx (Apache-2.0), changed: no i18n; the header is a real button; `workspaceId` is gone.
import { CaretRightIcon, ChatDotsIcon } from "@phosphor-icons/react";
import type { ReactNode } from "react";
import { cn } from "../lib/cn";

export interface ThinkingEntry {
  content: string;
  expansionKey: string;
}

export interface ChatCollapsedThinkingRenderProps {
  content: string;
  className?: string;
}

interface ChatCollapsedThinkingProps {
  entries: ThinkingEntry[];
  expanded: boolean;
  isHovered: boolean;
  onToggle: () => void;
  onHoverChange: (hovered: boolean) => void;
  className?: string;
  renderMarkdown: (props: ChatCollapsedThinkingRenderProps) => ReactNode;
}

/**
 * A collapsible group for thinking entries. When collapsed, shows "Thinking" with the thinking icon. When expanded,
 * shows all thinking entries with their full content.
 */
export function ChatCollapsedThinking({
  entries,
  expanded,
  isHovered,
  onToggle,
  onHoverChange,
  className,
  renderMarkdown,
}: ChatCollapsedThinkingProps) {
  if (entries.length === 0) return null;

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
        <span className="shrink-0 pt-0.5">
          {isHovered ? (
            <CaretRightIcon
              className={cn(
                "size-icon-base transition-transform duration-150",
                expanded && "rotate-90",
              )}
            />
          ) : (
            <ChatDotsIcon className="size-icon-base" />
          )}
        </span>
        <span className="truncate">Thinking</span>
      </button>

      {/* Expanded content */}
      {expanded && (
        <div className="ml-6 flex flex-col gap-base pt-1">
          {entries.map((entry) => (
            <div key={entry.expansionKey} className="pl-base text-sm text-low">
              {renderMarkdown({ content: entry.content, className: "text-sm" })}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
