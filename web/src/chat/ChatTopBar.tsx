/**
 * The bar on top of the chat column (the desktop app's CH3-D): "Chats (n) ▾", which drops down the list of chats, then
 * the title of the chat, its actions on the right, and "+ New". The list shows newest first, with the chosen chat
 * marked, how many wait for an answer and which have answers the person has not read.
 */
import { CaretDownIcon } from "@phosphor-icons/react";
import type { ReactNode } from "react";
import { Link } from "react-router";
import type { Chat } from "../api/types/chat";
import { Tag } from "../ui/chat/Tag";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "../ui/components/DropdownMenu";
import { cn } from "../ui/lib/cn";
import { ago, statusTag } from "./format";

interface ChatTopBarProps {
  /** The chats the page lists, newest first. */
  chats: Chat[];
  openId: number | null;
  now: number;
  /** The name of the place a chat works in, by chat id. */
  places: (chat: Chat) => string | null;
  asking: number;
  title?: ReactNode;
  actions?: ReactNode;
}

function ChatRow({
  chat,
  now,
  place,
  chosen,
}: {
  chat: Chat;
  now: number;
  place: string | null;
  chosen: boolean;
}) {
  const tag = statusTag(chat.status, chat.asks);
  const unread = chat.answered > chat.seen;
  const meta = [chat.agent, place].filter(Boolean).join(" · ");
  return (
    <DropdownMenuItem asChild>
      <Link
        to={`/chat/${chat.id}`}
        data-testid="chat-row"
        data-unread={unread ? "true" : undefined}
        aria-current={chosen ? "true" : undefined}
        className={cn(
          "flex flex-col items-stretch gap-half rounded-md border px-plusfifty py-base",
          chosen ? "border-text bg-lane" : "border-track bg-panel",
        )}
      >
        <span className="flex items-center gap-base">
          {tag && <Tag tone={tag.tone}>{tag.label}</Tag>}
          {unread && <Tag tone="amber">New</Tag>}
          <span className="flex-1" />
          <span className="font-mono text-xs text-low">{ago(now, chat.started_at)}</span>
        </span>
        <span className="truncate text-sm font-semibold text-high">{chat.title || "New chat"}</span>
        <span className="truncate font-mono text-xs text-normal">{meta}</span>
      </Link>
    </DropdownMenuItem>
  );
}

export function ChatTopBar({
  chats,
  openId,
  now,
  places,
  asking,
  title,
  actions,
}: ChatTopBarProps) {
  const pill =
    "inline-flex items-center gap-half rounded-full border border-line-strong bg-card px-plusfifty py-half text-sm font-semibold text-high transition-colors hover:bg-card-hover";
  return (
    <div className="flex shrink-0 items-center gap-base rounded-full border border-line-strong bg-panel px-base py-half shadow-md">
      <DropdownMenu>
        <DropdownMenuTrigger className={pill} aria-label={`Chats (${chats.length})`}>
          Chats ({chats.length})
          <CaretDownIcon className="size-icon-xs" />
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="w-[22rem] max-w-[90vw] p-base">
          <DropdownMenuLabel className="font-mono text-xs font-normal text-low">
            Recent conversations
          </DropdownMenuLabel>
          <div className="flex max-h-[26rem] flex-col gap-half overflow-y-auto">
            {chats.length === 0 && <p className="px-base py-base text-sm text-low">No chat yet.</p>}
            {chats.map((chat) => (
              <ChatRow
                key={chat.id}
                chat={chat}
                now={now}
                place={places(chat)}
                chosen={chat.id === openId}
              />
            ))}
          </div>
        </DropdownMenuContent>
      </DropdownMenu>
      {asking > 0 && <Tag tone="amber">{asking === 1 ? "1 asks you" : `${asking} ask you`}</Tag>}
      <div className="flex min-w-0 flex-1 items-center gap-base">{title}</div>
      {actions}
      <Link to="/chat" className={pill}>
        + New
      </Link>
    </div>
  );
}
