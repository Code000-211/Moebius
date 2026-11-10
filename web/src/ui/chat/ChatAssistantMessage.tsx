// From BloopAI/vibe-kanban packages/ui/src/components/ChatAssistantMessage.tsx (Apache-2.0), changed: `workspaceId` is
// gone; a Copy button comes with the message, given by the caller as `actions`; the answer is body text across the
// whole column rather than the small, narrow-capped text of the copied file.
import type { ReactNode } from "react";

export interface ChatAssistantMessageRenderProps {
  content: string;
}

interface ChatAssistantMessageProps {
  content: string;
  renderMarkdown: (props: ChatAssistantMessageRenderProps) => ReactNode;
  actions?: ReactNode;
}

export function ChatAssistantMessage({
  content,
  renderMarkdown,
  actions,
}: ChatAssistantMessageProps) {
  return (
    // The actions float over the corner of the message rather than taking a row under it (`copy-float`), so a message
    // is as tall as its words and does not end in a band of nothing while the button waits to be hovered. Where
    // nothing can be hovered the row stays in the flow and stays visible.
    <div className="reveal-copy relative flex flex-col gap-2">
      {renderMarkdown({ content })}
      {actions && <div className="copy-float flex">{actions}</div>}
    </div>
  );
}
