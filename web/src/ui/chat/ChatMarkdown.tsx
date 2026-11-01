// From BloopAI/vibe-kanban packages/ui/src/components/ChatMarkdown.tsx (Apache-2.0), changed: `workspaceId` is gone; the width is 100% of the column by default.
import type { ReactNode } from "react";
import { cn } from "../lib/cn";

export interface ChatMarkdownRenderProps {
  content: string;
  className?: string;
}

interface ChatMarkdownProps {
  content: string;
  maxWidth?: string;
  className?: string;
  renderContent: (props: ChatMarkdownRenderProps) => ReactNode;
}

export function ChatMarkdown({
  content,
  maxWidth = "100%",
  className,
  renderContent,
}: ChatMarkdownProps) {
  const contentClassName = cn("break-words", className);

  return (
    <div className="text-sm" style={{ maxWidth }}>
      {renderContent({ content, className: contentClassName })}
    </div>
  );
}
