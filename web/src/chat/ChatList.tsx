/**
 * The list of chats in a column of its own, to the left of the open one: its heading says how many there are and
 * whether one of them waits on you, then one row per chat, newest first, each opening its chat and each with its
 * Delete, which asks once. The chosen chat is marked and one with answers nobody has read says so. A row's name is one line that fades out where it runs short, the way the
 * title of the open chat does, and its whole name is in the tooltip and in the title row above the canvas. The column
 * is the only thing on the page that scrolls its list, and it collapses to nothing when the canvas is wanted on its
 * own; the choice is kept per browser.
 */
import { SidebarSimpleIcon, TrashIcon } from "@phosphor-icons/react";
import { useState } from "react";
import { Link } from "react-router";
import type { Chat } from "../api/types/chat";
import { Tag } from "../ui/chat/Tag";
import { Button } from "../ui/components/Button";
import { IconButton } from "../ui/components/IconButton";
import { StatusDot } from "../ui/components/StatusDot";
import { cn } from "../ui/lib/cn";
import { ago, plural } from "../ui/lib/format";
import { FadedLine } from "../ui/page/FadedLine";
import { DeleteChat } from "./DeleteChat";
import { statusTag } from "./format";
import { askingCount } from "./readMarkers";

interface ChatListProps {
  /** The chats the page lists, newest first. */
  chats: Chat[];
  openId: number | null;
  now: number;
  /** The name of the place a chat works in, by chat id. */
  places: (chat: Chat) => string | null;
  /** The person asked for the canvas on its own. */
  onCollapse: () => void;
  /** A chat was chosen from a row. */
  onChoose: () => void;
}

// The row is the box its words are in: the link carries the fill of the chosen chat and the padding, so nothing
// inside it can fall outside a row of its own.
function ChatRow({
  chat,
  now,
  place,
  chosen,
  onChoose,
}: {
  chat: Chat;
  now: number;
  place: string | null;
  chosen: boolean;
  onChoose: () => void;
}) {
  const tag = statusTag(chat.status, chat.asks);
  const unread = chat.answered > chat.seen;
  const name = chat.title || "New chat";
  const meta = [chat.agent, place].filter(Boolean).join(" · ");
  return (
    <Link
      to={`/chat/${chat.id}`}
      onClick={onChoose}
      data-testid="chat-row"
      data-unread={unread ? "true" : undefined}
      aria-current={chosen ? "true" : undefined}
      className={cn(
        "flex min-w-0 flex-col gap-2 rounded-control px-3 py-2 text-left [@media(hover:none)]:pr-12",
        chosen ? "bg-selected" : "hover:bg-hover",
      )}
    >
      <span className="flex flex-wrap items-center gap-2">
        {tag && <Tag tone={tag.tone}>{tag.label}</Tag>}
        {unread && (
          <>
            <StatusDot color="var(--attention)" data-testid="unread-dot" />
            <span className="sr-only">New answers</span>
          </>
        )}
        <FadedLine
          text={name}
          keep="head"
          title={name}
          className="flex-1 text-base font-semibold text-text"
        />
      </span>
      <span className="flex flex-wrap items-center gap-2 text-sm text-secondary">
        <span className="min-w-0 break-words">{meta}</span>
        <span className="shrink-0 tabular-nums">{ago(now, chat.started_at)}</span>
      </span>
    </Link>
  );
}

export function ChatList({ chats, openId, now, places, onCollapse, onChoose }: ChatListProps) {
  /** The chat whose delete is being asked about, from a row of the list. */
  const [deleting, setDeleting] = useState<Chat | null>(null);
  const asking = askingCount(chats);
  return (
    <aside
      id="chat-list"
      aria-label="Chats"
      // Below a wide screen the column lies over the chat rather than squeezing it: a composer of eight characters
      // is no composer at all.
      className="absolute inset-y-0 left-0 z-20 flex h-full w-64 shrink-0 flex-col gap-2 border-r border-hairline bg-background pr-3 lg:static lg:z-auto"
    >
      {/* The name, the count, and a dot for a chat that waits on you — the same dot the top bar's switcher carries
          while the column is hidden. The way back to the canvas on its own is at the end of the row. */}
      <div className="flex shrink-0 items-center gap-2">
        <h2 className="flex min-w-0 flex-1 items-center gap-2 text-base font-semibold text-text">
          Chats
          <span className="text-sm font-normal tabular-nums text-secondary">{chats.length}</span>
          {asking > 0 && (
            <>
              <StatusDot color="var(--attention)" />
              <span className="sr-only">{plural(asking, "chat asks you", "chats ask you")}</span>
            </>
          )}
        </h2>
        <IconButton
          icon={SidebarSimpleIcon}
          aria-label="Hide the list of chats"
          onClick={onCollapse}
        />
      </div>
      {/* The one scroller of the list, so a row is never squeezed by a box inside it. */}
      <div
        data-testid="chat-list-rows"
        className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto pb-2"
      >
        {chats.length === 0 && (
          <p className="px-3 py-2 text-sm text-secondary">
            No chat yet. Write below and the agent starts one.
          </p>
        )}
        {chats.map((chat) => (
          <div key={chat.id} className="group relative shrink-0">
            <ChatRow
              chat={chat}
              now={now}
              place={places(chat)}
              chosen={chat.id === openId}
              onChoose={onChoose}
            />
            {/* The Delete lies over the row and is only there while the row is looked at, or while the keyboard has
                reached the Delete itself, so the words have the column to themselves. Picking a chat leaves the focus
                on its row, which is no reason to cover its words. It takes no pointer while out of sight, so the
                empty corner of a row still opens its chat. A screen that cannot hover has no way to look at a row
                without opening it, so there the Delete is always shown and the row leaves room for it. */}
            <Button
              variant="danger"
              size="icon"
              aria-label={`Delete chat ${chat.title || chat.id}`}
              title="Delete"
              onClick={() => setDeleting(chat)}
              className="pointer-events-none absolute top-1/2 right-1 -translate-y-1/2 opacity-0 group-hover:pointer-events-auto group-hover:opacity-100 focus-visible:pointer-events-auto focus-visible:opacity-100 [@media(hover:none)]:pointer-events-auto [@media(hover:none)]:opacity-100"
            >
              <TrashIcon aria-hidden="true" className="size-5" weight="bold" />
            </Button>
          </div>
        ))}
      </div>
      {deleting && (
        <DeleteChat
          chat={deleting}
          open
          onOpenChange={(open) => {
            if (!open) setDeleting(null);
          }}
          openId={openId}
        />
      )}
    </aside>
  );
}
