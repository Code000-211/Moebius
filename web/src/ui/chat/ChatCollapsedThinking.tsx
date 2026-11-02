// From BloopAI/vibe-kanban packages/ui/src/components/ChatCollapsedThinking.tsx (Apache-2.0), changed: no i18n; the
// row is a real button 40px tall with a readable label, and the thinking it holds is body text at the size of the
// conversation rather than a muted line; `workspaceId` is gone.
import { CaretRightIcon } from "@phosphor-icons/react";
import type { ReactNode } from "react";
import { cn } from "../lib/cn";
import { elapsed } from "../lib/format";

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
  onToggle: () => void;
  className?: string;
  renderMarkdown: (props: ChatCollapsedThinkingRenderProps) => ReactNode;
  /** How long the thinking took, in seconds; null while the model is still at it or when nothing came after it. */
  seconds?: number | null;
  /** Whether the model is thinking this one now. */
  ongoing?: boolean;
}

/**
 * The word for a thought and, once it has stopped, how long it took: "Thought 12s". A thought still arriving is said
 * in the present tense and without a length, because it has none yet.
 */
export function thinkingLabel(seconds: number | null | undefined, ongoing: boolean): string {
  if (ongoing) return "Thinking";
  if (seconds === null || seconds === undefined || seconds < 1) return "Thought";
  return `Thought ${elapsed(seconds)}`;
}

/**
 * A collapsible group for thinking entries. Collapsed it is one 40px row that says what it was and how long it
 * took, at the size of the other steps of the turn so that a thought does not shout over the answer it led to; opened,
 * it is the thinking itself, as prose that is quieter than the answer but no smaller. The caret that says the row opens
 * is always drawn and turns with the state, so the affordance does not depend on a mouse being over it.
 */
export function ChatCollapsedThinking({
  entries,
  expanded,
  onToggle,
  className,
  renderMarkdown,
  seconds = null,
  ongoing = false,
}: ChatCollapsedThinkingProps) {
  if (entries.length === 0) return null;

  return (
    <div className={cn("flex max-w-prose flex-col", className)}>
      <button
        type="button"
        className="flex min-h-8 cursor-pointer items-center gap-2 self-start rounded-control px-3 text-left text-sm text-secondary transition-colors duration-(--motion-routine) ease-out hover:bg-hover hover:text-text"
        onClick={onToggle}
        aria-expanded={expanded}
      >
        <CaretRightIcon
          aria-hidden="true"
          className={cn(
            "size-4 shrink-0 transition-transform duration-(--motion-routine) ease-out",
            expanded && "rotate-90",
          )}
        />
        {thinkingLabel(seconds, ongoing)}
      </button>

      {expanded && (
        <div className="flex flex-col gap-4 py-2 pl-4">
          {entries.map((entry) => (
            <div key={entry.expansionKey} className="border-l-2 border-hairline pl-4">
              {renderMarkdown({ content: entry.content, className: "text-secondary" })}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
