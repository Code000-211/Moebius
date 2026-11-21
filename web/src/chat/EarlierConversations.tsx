/**
 * The earlier conversations of the Moebius agent, as a short list inside its panel: the chats of origin `assistant`
 * that the person started, newest first, each a quiet row with the first words it said and how long ago. A row opens
 * the conversation in the panel. The panel keeps them so that past conversations have a home without a place of their
 * own in the sidebar; the way here is one icon in the panel's head, and the way back is the arrow that replaces the
 * mark of Moebius there.
 */
import type { Chat } from "../api/types/chat";
import { compactAge } from "../ui/lib/format";
import { useNow } from "../ui/lib/useNow";
import { Empty } from "../ui/page/Empty";

/** The conversations to list: the person's own with the agent, the newest first. */
export function earlier(chats: Chat[] | undefined): Chat[] {
  return (chats ?? [])
    .filter((chat) => chat.origin === "assistant" && chat.mine)
    .sort((a, b) => b.started_at - a.started_at);
}

export function EarlierConversations({
  chats,
  current,
  onOpen,
}: {
  chats: Chat[];
  /** The conversation open in the panel, which is marked. */
  current: number | null;
  onOpen: (chat: Chat) => void;
}) {
  const now = useNow();
  if (chats.length === 0) {
    return (
      <Empty
        plain
        title="No earlier conversations"
        body="What you ask the Moebius agent is kept here."
      />
    );
  }
  return (
    <ul
      className="m-0 flex min-h-0 flex-1 list-none flex-col overflow-y-auto p-0"
      data-testid="earlier"
    >
      {chats.map((chat) => (
        <li key={chat.id}>
          <button
            type="button"
            aria-current={chat.id === current ? "true" : undefined}
            onClick={() => onOpen(chat)}
            className="flex min-h-9 w-full cursor-pointer items-center gap-2 rounded-control px-2 text-left text-md text-text transition-colors duration-(--motion-routine) ease-out hover:bg-hover aria-[current=true]:bg-hover"
          >
            <span className="min-w-0 flex-1 truncate">{chat.title || "Untitled conversation"}</span>
            <span className="shrink-0 text-sm tabular-nums text-muted">
              {compactAge(now, chat.started_at)}
            </span>
          </button>
        </li>
      ))}
    </ul>
  );
}
