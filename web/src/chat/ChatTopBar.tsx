/**
 * The bar on top of the chat column (the desktop app's CH3-D): "Chats" with a quiet count, which drops down the list
 * of chats, then
 * the title of the chat, its actions on the right, and the way to a new one. The list is newest first, with the chosen
 * chat marked, and which have answers the person has not read, and it scrolls once — the menu's own surface. A dot beside the word says that one of them waits on
 * you, because that is a fact about the list rather than about the chat on screen. It is a row of controls over the
 * canvas rather than a panel of its own, and every control in it is the shared `Button`, at the size the header
 * of every other page uses, so the row of controls under a title is the same row wherever it is met.
 */
import { CaretDownIcon, PlusIcon } from "@phosphor-icons/react";
import type { ReactNode } from "react";
import { Link } from "react-router";
import type { Chat } from "../api/types/chat";
import { Tag } from "../ui/chat/Tag";
import { Button, buttonVariants } from "../ui/components/Button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "../ui/components/DropdownMenu";
import { StatusDot } from "../ui/components/StatusDot";
import { cn } from "../ui/lib/cn";
import { ago, plural } from "../ui/lib/format";
import { statusTag } from "./format";

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

// The dropdown opens from a button like every other one on a page header, so it wears `Button`'s own classes
// rather than a copy of them.
const TRIGGER = buttonVariants({ variant: "secondary" });

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
  // The row's shape is asked of the item, not of the link inside it: the item merges what it is given with the
  // classes every menu item carries, so the two lines of the row are one box. Asked of the link instead, both
  // sets reached the document and the row kept the shared centred line while drawing its own fill — too short for
  // its words, which then fell out of it.
  return (
    <DropdownMenuItem asChild className="flex-col items-stretch gap-2 py-2">
      <Link
        to={`/chat/${chat.id}`}
        data-testid="chat-row"
        data-unread={unread ? "true" : undefined}
        aria-current={chosen ? "true" : undefined}
        className={cn("text-left", chosen ? "bg-selected" : "hover:bg-hover")}
      >
        <span className="flex flex-wrap items-center gap-2">
          {tag && <Tag tone={tag.tone}>{tag.label}</Tag>}
          {unread && <Tag tone="attention">New</Tag>}
          <span className="min-w-0 flex-1 break-words text-base font-semibold text-text">
            {chat.title || "New chat"}
          </span>
        </span>
        <span className="flex flex-wrap items-center gap-2 text-sm text-secondary">
          <span className="min-w-0 break-words">{meta}</span>
          <span className="shrink-0 tabular-nums">{ago(now, chat.started_at)}</span>
        </span>
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
  // One chat waiting on an answer is said by a dot beside the switcher, not by a pill beside the title: the title
  // already says what the open chat is doing, and the rows inside name each one that asks.
  const said =
    asking > 0
      ? `Chats (${chats.length}). ${plural(asking, "chat asks you", "chats ask you")}`
      : `Chats (${chats.length})`;
  return (
    <div className="flex shrink-0 flex-wrap items-center gap-2">
      <DropdownMenu>
        <DropdownMenuTrigger className={TRIGGER} aria-label={said}>
          {asking > 0 && <StatusDot color="var(--attention)" />}
          Chats
          <span className="text-sm font-normal tabular-nums text-secondary">{chats.length}</span>
          <CaretDownIcon aria-hidden="true" className="size-4" />
        </DropdownMenuTrigger>
        <DropdownMenuContent
          align="start"
          className="w-96 max-w-(--radix-dropdown-menu-content-available-width)"
        >
          <DropdownMenuLabel>Recent chats</DropdownMenuLabel>
          {/* The menu itself is the one scroller, at the height the space under the trigger allows. A second one in
              here capped the list's height instead, and a column of a fixed height shrinks the rows it holds — a
              row is allowed down to its own `min-h-10` — so the words of every row hung below the row's fill. */}
          <div className="flex flex-col gap-2">
            {chats.length === 0 && (
              <p className="px-3 py-2 text-sm text-secondary">
                No chat yet. Write below and the agent starts one.
              </p>
            )}
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
      <div className="flex min-w-0 flex-1 items-center gap-2">{title}</div>
      {actions}
      <Button asChild variant="primary">
        <Link to="/chat">
          <PlusIcon className="size-4" weight="bold" aria-hidden="true" />
          New
        </Link>
      </Button>
    </div>
  );
}
