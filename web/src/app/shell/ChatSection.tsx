/**
 * The list of the conversations in the wide sidebar, under the day each was started, with the way to start a new one,
 * the project dropdown and the field that narrow it. `Sidebar` stands it below the project's places and Improve.
 */

import { CaretDownIcon, PlusIcon } from "@phosphor-icons/react";
import { useMemo, useState } from "react";
import { Link } from "react-router";
import type { Snapshot } from "../../api/types/board";
import type { Chat } from "../../api/types/chat";
import {
  type ChatFilter,
  type ChatScope,
  filterChats,
  formatChatFilter,
  groupChats,
  parseChatFilter,
} from "../../chat/chatList";
import { placeName } from "../../chat/places";
import { Button } from "../../ui/components/Button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "../../ui/components/DropdownMenu";
import { StatusDot } from "../../ui/components/StatusDot";
import { cn } from "../../ui/lib/cn";
import { readText, writeText } from "../storage";
import { Section } from "./SidebarSection";

/** Where the chosen project of the list is kept for this browser (`moebius.sidebar.*`). */
const CHAT_FILTER_KEY = "moebius.sidebar.chatsRepository";

/**
 * The list of the conversations, under Projects and Improve, in the wide sidebar only: the rail has no room for a
 * list of them,
 * so it keeps its one Chat entry instead. The chats stand under the day each was started (`groupChats`), each row
 * with its unread dot and the place it works in, and the one on screen marked by its own address the way a page is.
 * Above the list, the way to start a new one sits beside the compact dropdown and the field that narrow the list: All
 * projects, a repository of the snapshot, or Without project — kept in this browser so the next visit reads the same
 * list; the field narrows what the dropdown leaves by title while it is typed. They change nothing on the server, and
 * neither duplicates the search of the command bar above, which looks everywhere else.
 */
export function ChatSection({
  chats,
  snapshot,
  at,
}: {
  chats: Chat[];
  snapshot: Snapshot | undefined;
  at: string;
}) {
  const [filter, setFilter] = useState<ChatFilter>(() =>
    parseChatFilter(readText(CHAT_FILTER_KEY)),
  );
  const [query, setQuery] = useState("");
  const [scope, setScope] = useState<ChatScope>("mine");
  const choose = (next: ChatFilter) => {
    setFilter(next);
    writeText(CHAT_FILTER_KEY, formatChatFilter(next));
  };
  const shown = useMemo(
    () => filterChats(chats, snapshot, filter, query, scope),
    [chats, snapshot, filter, query, scope],
  );
  const groups = groupChats(shown, Math.floor(Date.now() / 1000));
  const named =
    filter === "all"
      ? "All projects"
      : filter === "none"
        ? "Without project"
        : (snapshot?.boards.find((board) => board.repository.id === filter)?.repository.name ??
          "All projects");
  return (
    <Section title="Chats" collapsed={false}>
      <li className="flex items-center gap-1">
        {/* The compact way to a new chat is a bare `+`: its place in the row and the accessible name say enough. */}
        <Button asChild variant="icon" size="sm" title="New chat">
          <Link to="/chat" aria-label="Start a chat in the sidebar">
            <PlusIcon className="size-4" weight="bold" aria-hidden="true" />
          </Link>
        </Button>
        <input
          type="search"
          aria-label="Filter the chats"
          placeholder="Filter"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          className="w-16 min-w-0 shrink-0 rounded-control bg-sunken px-2 py-1 text-sm text-text outline-none transition-colors duration-(--motion-routine) ease-out placeholder:text-secondary focus-visible:bg-background focus-visible:outline-1 focus-visible:-outline-offset-1 focus-visible:outline-hairline-strong"
        />
        <DropdownMenu modal={false}>
          <DropdownMenuTrigger asChild>
            {/* Bare like the `+` beside it: the label and the caret say enough, the hover does the rest. */}
            <Button
              variant="ghost"
              size="sm"
              className="min-w-0 max-w-32 flex-1"
              aria-label="Filter chats by project"
            >
              <span className="min-w-0 truncate">{named}</span>
              <CaretDownIcon className="size-4 shrink-0" weight="bold" aria-hidden="true" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuRadioGroup
              value={formatChatFilter(filter)}
              onValueChange={(next) => choose(parseChatFilter(next))}
            >
              <DropdownMenuRadioItem value="all">All projects</DropdownMenuRadioItem>
              {snapshot?.boards.map((board) => (
                <DropdownMenuRadioItem
                  key={board.repository.id}
                  value={String(board.repository.id)}
                >
                  {board.repository.name}
                </DropdownMenuRadioItem>
              ))}
              <DropdownMenuRadioItem value="none">Without project</DropdownMenuRadioItem>
            </DropdownMenuRadioGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      </li>
      <li className="flex gap-1">
        {(["mine", "shared"] as const).map((each) => (
          <Button
            key={each}
            variant={scope === each ? "secondary" : "ghost"}
            size="sm"
            aria-pressed={scope === each}
            onClick={() => setScope(each)}
          >
            {each === "mine" ? "Mine" : "Shared"}
          </Button>
        ))}
      </li>
      {shown.length === 0 && chats.length > 0 && (
        <li>
          <p className="px-3 py-2 text-sm text-secondary">No chats match</p>
        </li>
      )}
      {groups.map((group) => (
        <div key={group.label} className="mt-1 flex flex-col gap-1">
          <div
            className="px-2 text-xs font-semibold text-sidebar-group"
            data-testid="chat-group-label"
          >
            {group.label}
          </div>
          {group.chats.map((chat) => (
            <ChatEntry
              key={chat.id}
              chat={chat}
              snapshot={snapshot}
              current={at === `/chat/${chat.id}`}
            />
          ))}
        </div>
      ))}
    </Section>
  );
}

/** One conversation of the list: its name, the place it works in, and a dot while it has answers nobody read. */
function ChatEntry({
  chat,
  snapshot,
  current,
}: {
  chat: Chat;
  snapshot: Snapshot | undefined;
  current: boolean;
}) {
  const name = chat.title || "New chat";
  const place = placeName(snapshot, chat);
  const unread = chat.answered > chat.seen;
  return (
    <li>
      <Link
        to={`/chat/${chat.id}`}
        aria-label={name}
        aria-current={current ? "true" : undefined}
        data-testid="chat-entry"
        data-unread={unread ? "true" : undefined}
        className={cn(
          "flex min-h-8 items-center gap-2 rounded-control px-3 text-base",
          current
            ? "bg-selected font-semibold text-text"
            : "text-sidebar-text hover:bg-hover hover:text-text",
        )}
      >
        <span className="min-w-0 flex-1 truncate" title={name}>
          {name}
        </span>
        {place && <span className="shrink-0 truncate text-xs text-secondary">{place}</span>}
        {unread && (
          <>
            <StatusDot color="var(--attention)" />
            <span className="sr-only">New answers</span>
          </>
        )}
      </Link>
    </li>
  );
}
