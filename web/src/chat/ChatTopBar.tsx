/**
 * The bar on top of the chat column (the former desktop app's CH3-D): the title of the chat, its actions on the right, and
 * the way to a new one. The column of chats beside it is not a menu dropped from here, and while it is on the screen
 * its own heading says how many chats there are, so this bar only carries the switcher that brings the column back
 * once it has been hidden; the dot on it says that one of them waits on you, because that is a fact about the list
 * rather than about the chat on screen. It is a row of controls over the canvas rather than a panel of its own, and
 * every control in it is the shared `Button`, at the size the header of every other page uses, so the row of controls
 * under a title is the same row wherever it is met.
 */
import { PlusIcon, SidebarSimpleIcon } from "@phosphor-icons/react";
import type { ReactNode } from "react";
import { Link } from "react-router";
import type { Chat } from "../api/types/chat";
import { Button } from "../ui/components/Button";
import { StatusDot } from "../ui/components/StatusDot";
import { plural } from "../ui/lib/format";
import { askingCount } from "./readMarkers";

interface ChatTopBarProps {
  /** The chats the page lists, newest first. */
  chats: Chat[];
  /** Whether the column of chats beside the canvas is shown. */
  listOpen: boolean;
  /** The person asked for the other one. */
  onToggleList: () => void;
  title?: ReactNode;
  actions?: ReactNode;
}

export function ChatTopBar({ chats, listOpen, onToggleList, title, actions }: ChatTopBarProps) {
  const asking = askingCount(chats);
  // One chat waiting on an answer is said by a dot beside the switcher, not by a pill beside the title: the title
  // already says what the open chat is doing, and the rows of the list name each one that asks.
  const said =
    asking > 0
      ? `Chats (${chats.length}). ${plural(asking, "chat asks you", "chats ask you")}`
      : `Chats (${chats.length})`;
  return (
    <div className="flex shrink-0 flex-wrap items-center gap-2">
      {/* Only while the column is out of the way: with it on the screen the heading at its top says the same, and a
          button that repeats the heading beside it is one control too many. */}
      {listOpen ? null : (
        <Button variant="secondary" aria-label={said} onClick={onToggleList}>
          {asking > 0 && <StatusDot color="var(--attention)" />}
          Chats
          <span className="text-sm font-normal tabular-nums text-secondary">{chats.length}</span>
          <SidebarSimpleIcon aria-hidden="true" className="size-4" />
        </Button>
      )}
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
