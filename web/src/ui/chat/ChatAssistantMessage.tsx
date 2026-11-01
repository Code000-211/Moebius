// From BloopAI/vibe-kanban packages/ui/src/components/ChatAssistantMessage.tsx (Apache-2.0), changed: `workspaceId` is gone; a Copy button sits under the message, given by the caller as `actions`.
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
    <div className="group/message flex flex-col gap-half">
      {renderMarkdown({ content })}
      {actions && <div className="flex">{actions}</div>}
    </div>
  );
}
