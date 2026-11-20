// From BloopAI/vibe-kanban packages/ui/src/components/ChatUserMessage.tsx (Apache-2.0), changed: no i18n and no edit or
// reset (the server keeps no way to rewrite a message); the message is a bubble on the right of the column rather than
// a titled band; a steering message says so in a small line above it; `copy` takes the Copy button; no `workspaceId`.
import type { ReactNode } from "react";
import { cn } from "../lib/cn";

export interface ChatUserMessageRenderProps {
  content: string;
}

interface ChatUserMessageProps {
  content: string;
  /** Whether the message steered a chat that was at work, which is said above it. */
  steered?: boolean;
  className?: string;
  /** The Copy button, drawn beside the bubble. */
  copy?: ReactNode;
  renderMarkdown: (props: ChatUserMessageRenderProps) => ReactNode;
}

export function ChatUserMessage({
  content,
  steered = false,
  className,
  copy,
  renderMarkdown,
}: ChatUserMessageProps) {
  return (
    <div className={cn("reveal-copy flex min-w-0 flex-col items-end gap-1", className)}>
      {steered && <span className="text-xs text-secondary">You steered the chat</span>}
      <div className="relative flex max-w-[85%] min-w-0 items-center gap-1">
        {/* On a screen that can hover, Copy floats to the left of the bubble, so it never covers the words. */}
        {copy && (
          <div className="copy-float top-1/2! right-full! bottom-auto! mr-1 flex shrink-0 -translate-y-1/2">
            {copy}
          </div>
        )}
        <div className="min-w-0 rounded-card bg-hover px-3 py-2">{renderMarkdown({ content })}</div>
      </div>
    </div>
  );
}
