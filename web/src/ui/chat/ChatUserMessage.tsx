// From BloopAI/vibe-kanban packages/ui/src/components/ChatUserMessage.tsx (Apache-2.0), changed: no i18n and no edit or
// reset (the server keeps no way to rewrite a message); the title says "You" or "You steered"; `headerRight` takes the
// Copy button; no `workspaceId`.
import type { ReactNode } from "react";
import { ChatEntryContainer } from "./ChatEntryContainer";

export interface ChatUserMessageRenderProps {
  content: string;
}

interface ChatUserMessageProps {
  content: string;
  /** Who wrote it. The default is what a message of the person is called. */
  title?: string;
  expanded?: boolean;
  onToggle?: () => void;
  className?: string;
  isGreyed?: boolean;
  headerRight?: ReactNode;
  renderMarkdown: (props: ChatUserMessageRenderProps) => ReactNode;
}

export function ChatUserMessage({
  content,
  title = "You",
  expanded = true,
  onToggle,
  className,
  isGreyed,
  headerRight,
  renderMarkdown,
}: ChatUserMessageProps) {
  return (
    <ChatEntryContainer
      variant="user"
      title={title}
      expanded={expanded}
      onToggle={onToggle}
      className={className}
      isGreyed={isGreyed}
      headerRight={headerRight}
    >
      {renderMarkdown({ content })}
    </ChatEntryContainer>
  );
}
