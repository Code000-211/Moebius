// From BloopAI/vibe-kanban packages/ui/src/components/ChatMarkdown.tsx (Apache-2.0), changed: `workspaceId` is gone;
// the measure is the one of the standard (`max-w-prose`) instead of a width the caller passes in pixels.
import type { ReactNode } from "react";
import { cn } from "../lib/cn";

export interface ChatMarkdownRenderProps {
  content: string;
  className?: string;
}

interface ChatMarkdownProps {
  content: string;
  className?: string;
  renderContent: (props: ChatMarkdownRenderProps) => ReactNode;
}

export function ChatMarkdown({ content, className, renderContent }: ChatMarkdownProps) {
  const contentClassName = cn("break-words", className);

  return (
    <div className="max-w-prose">{renderContent({ content, className: contentClassName })}</div>
  );
}
