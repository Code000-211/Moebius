/**
 * The sidebar on every page: the app name with the connection, the search (the command bar), Inbox, Fleet,
 * the folding groups Automation, Projects (one entry per board) and Improve, the list of chats below them in the
 * wide sidebar, then Setup and the one entry for Settings. It collapses
 * to a rail of icons, where the words the rail drops — the connection, a board's work — stay with the icon as text a
 * person can hear or open, at the same heights the wide sidebar uses so that the rail is the same list seen close,
 * and where nothing folds. The counts come from the snapshot. The rail alone keeps the Chat entry; the wide list
 * leaves it to the chat list in the sidebar.
 *
 * The pages that set the server up rather than run the work — General, Models, Plugins, Terminal, Ports, Usage, Rules,
 * Agents, Skills and MCP servers — are not listed beside the everyday pages. Choosing Settings swaps this list for
 * theirs, with a Back row above Search that returns to the page they left; the routes do not change, so a bookmark, a search and the back
 * button all still work. Signing out of this browser is an item of that list rather than a place, so it is a row that
 * does something instead of a page.
 */

import {
  ArrowLeftIcon,
  CaretDownIcon,
  FolderIcon,
  GearIcon,
  type Icon,
  MagnifyingGlassIcon,
  PlusIcon,
  SidebarSimpleIcon,
  SignOutIcon,
  SquaresFourIcon,
} from "@phosphor-icons/react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useState } from "react";
import { Link, useLocation } from "react-router";
import { snapshot } from "../../api/calls/board";
import { chats as readChats } from "../../api/calls/chat";
import { start } from "../../api/calls/server";
import type { ConnectionState } from "../../api/events";
import { keys } from "../../api/keys";
import type { Snapshot } from "../../api/types/board";
import { liveRuns } from "../../api/types/board";
import type { Chat } from "../../api/types/chat";
import {
  type ChatFilter,
  filterChats,
  formatChatFilter,
  groupChats,
  parseChatFilter,
  recentChat,
} from "../../chat/chatList";
import { placeName } from "../../chat/places";
import { unreadCount } from "../../chat/readMarkers";
import { Button } from "../../ui/components/Button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "../../ui/components/DropdownMenu";
import { IconButton } from "../../ui/components/IconButton";
import { Mark } from "../../ui/components/Mark";
import { StatusDot } from "../../ui/components/StatusDot";
import { Tooltip } from "../../ui/components/Tooltip";
import { cn } from "../../ui/lib/cn";
import { getModifierKey } from "../../ui/lib/platform";
import { ScrollFades } from "../../ui/page/ScrollFades";
import {
  boardPath,
  DEFAULT_PAGE,
  type Group,
  isSettingsPage,
  PAGES,
  type PageDef,
  pageByPath,
  SETTINGS_PAGES,
  settingsLabel,
} from "../pages";
import { readFlagOr, readText, writeFlag, writeText } from "../storage";
import { leaveSession } from "./session";

const STATE: Record<ConnectionState, { label: string; color: string }> = {
  online: { label: "Online", color: "var(--complete)" },
  connecting: { label: "Connecting", color: "var(--text-secondary)" },
  reconnecting: { label: "Reconnecting", color: "var(--attention)" },
};

/** Where the chosen project of the list is kept for this browser (`moebius.sidebar.*`). */
const CHAT_FILTER_KEY = "moebius.sidebar.chatsRepository";

/** How the words after an entry's name are coloured: a count that waits, work that is running, or the plain state. */
type TrailingTone = "attention" | "progress" | "secondary";

/**
 * The groups that fold. Automation holds Automations and Workflows; Improve holds Retro, Experiments, Decisions and
 * Usage; Projects is folded by its own section, which keeps the row of every repository the snapshot names rather
 * than one page to fold over. Whether each is open is kept per browser under its own key, like the sidebar's own
 * shape is; a browser that has never chosen anything has every group open. In the rail the groups do not fold —
 * there a label keeps the room it always took and every row under it stays where it was.
 */
const COLLAPSIBLE: readonly { key: string; title: string; paths: readonly string[] }[] = [
  { key: "group.automation", title: "Automation", paths: ["automations", "workflows"] },
  {
    key: "group.improve",
    title: "Improve",
    paths: ["retro", "experiments", "decision-model", "usage"],
  },
  { key: "group.projects", title: "Projects", paths: [] },
];

/** The words after an entry's name: the count or state of what it leads to. Colour supports them, never replaces them. */
interface Trailing {
  text: string;
  tone: TrailingTone;
}

/**
 * The mark at the end of a row that stands for the state of what it leads to.
 *
 * A state almost every row is in is a mark and no word: a list of repositories that are all waiting says "Idle" as
 * many times as it has rows, and the word that repeats is the one nobody reads. The word is kept where a person
 * cannot see the colour — in the name of the row, and in the rail, where the words themselves are gone.
 */
interface RowState {
  color: string;
  word: string;
  /** The colour of the word, when it is printed. */
  tone: TrailingTone;
  /** Whether the word stands beside the mark, as a count does, or stays quiet, as a state every row shares does. */
  printed: boolean;
}

/** The fill of each tone. */
const TRAILING: Record<TrailingTone, string> = {
  attention: "text-attention-text",
  progress: "text-progress-text",
  secondary: "text-secondary",
};

interface SidebarProps {
  connection: ConnectionState;
  collapsed: boolean;
  onToggle: () => void;
  onSearch: () => void;
}

export function Sidebar({ connection, collapsed, onToggle, onSearch }: SidebarProps) {
  const data = useQuery({ queryKey: keys.snapshot, queryFn: () => snapshot() });
  const chatList = useQuery({ queryKey: keys.chats, queryFn: readChats });
  const steps = useQuery({ queryKey: keys.start, queryFn: () => start() });
  const unread = unreadCount(chatList.data ?? []);
  const boards = data.data?.boards ?? [];
  const location = useLocation();
  // The sidebar shows the pages of Settings while one of them is on screen, and the everyday pages otherwise.
  const here = location.pathname.replace(/^\/+|\/+$/g, "").split("/")[0] ?? "";
  const at = location.pathname;
  const inSettings = isSettingsPage(here);
  // Where "back" goes: the last everyday page this sidebar was opened on.
  const [lastPage, setLastPage] = useState(`/${DEFAULT_PAGE}`);
  useEffect(() => {
    if (!inSettings && location.pathname !== "/") setLastPage(location.pathname);
  }, [inSettings, location.pathname]);
  // Chat leads to the conversation, not to an empty field: the most recent one when there is one, and a new chat
  // when the server has none yet.
  const recent = recentChat(chatList.data ?? []);
  const inbox = data.data?.inbox.length ?? 0;
  const working = boards.reduce((sum, board) => sum + liveRuns(board), 0);
  const state = STATE[connection];
  const modifier = getModifierKey();
  // Whether a group that folds is open. A browser that has never chosen anything has every group open.
  const [openGroups, setOpenGroups] = useState<Record<string, boolean>>(() =>
    Object.fromEntries(
      COLLAPSIBLE.map((group) => [group.key, readFlagOr(`moebius.sidebar.${group.key}`, true)]),
    ),
  );
  const toggleGroup = (key: string, open: boolean) => {
    setOpenGroups((openNow) => ({ ...openNow, [key]: open }));
    writeFlag(`moebius.sidebar.${key}`, open);
  };
  // Whether the group of Projects is open; its rows are the boards the snapshot names, not pages of a list.
  const projectsOpen = openGroups["group.projects"] ?? true;

  // "Get started" is listed only while the server says a step is missing.
  const pagesOf = (group: Group) =>
    PAGES.filter(
      (page) =>
        page.group === group &&
        !isSettingsPage(page.path) &&
        // Chat is a row of the rail alone once the chat list lives in the sidebar: the wide list does not name it,
        // and the rail keeps its single icon leading to the newest conversation.
        (page.path !== "chat" || collapsed) &&
        (page.path !== "start" || (steps.data && !steps.data.done)),
    );
  // What waits for you is counted even when the answer is none — a zero on Inbox and Chat is the answer to the
  // question the page is asked — and the row's own name says what is counted, so the number stands alone. What the
  // machines are doing is said only when something is, and the number carries its colour: the row is about this
  // server's work, so "2" on Fleet needs no word after it.
  const trailingOf = (path: string): Trailing | undefined => {
    if (path === "inbox") return { text: `${inbox}`, tone: inbox > 0 ? "attention" : "secondary" };
    if (path === "chat") return { text: `${unread}`, tone: unread > 0 ? "attention" : "secondary" };
    if (path === "fleet" && working > 0) return { text: `${working}`, tone: "progress" };
    return undefined;
  };

  /** One row of the everyday list, from the page it leads to. */
  const entryOf = (page: PageDef) => (
    <Entry
      key={page.path}
      to={page.path === "chat" && recent !== null ? `/chat/${recent.id}` : `/${page.path}`}
      // A chat has an address of its own, so the entry stays standing on every conversation rather than only
      // on the one it happens to lead to.
      current={page.path === "chat" ? here === "chat" : here === page.path}
      icon={page.icon}
      label={page.title}
      collapsed={collapsed}
      trailing={trailingOf(page.path)}
    />
  );

  /** One flat section of the everyday list; nothing when its group has no entry to show. */
  const groupSection = (group: Group, title?: string) => {
    // The pages a folding group holds are the folding group's to show, not the flat section's.
    const folded = new Set(COLLAPSIBLE.flatMap((each) => each.paths));
    const pages = pagesOf(group).filter((page) => !folded.has(page.path));
    if (pages.length === 0) return null;
    return (
      <Section title={title} collapsed={collapsed}>
        {pages.map(entryOf)}
      </Section>
    );
  };

  /** One folding group: the toggle is the caret beside its name, and its pages stand under it only while it is open. */
  const collapsibleSection = (title: string) => {
    const group = COLLAPSIBLE.find((each) => each.title === title);
    if (!group) return null;
    const open = openGroups[group.key] ?? true;
    const pages = group.paths.map(pageByPath).filter((page): page is PageDef => page !== undefined);
    return (
      <Section
        title={title}
        collapsed={collapsed}
        collapsible={{ open, onToggle: () => toggleGroup(group.key, !open) }}
      >
        {/* The name stays a label that goes nowhere; the caret beside it is the toggle (`docs/design-web.md`). */}
        {/* In the rail a group never folds, so its rows show whatever the browser left it as. */}
        {(open || collapsed) && pages.map((page) => entryOf(page))}
      </Section>
    );
  };

  return (
    <nav
      aria-label="Pages"
      className={cn(
        "flex h-full shrink-0 flex-col bg-sidebar",
        collapsed ? "w-(--rail-width)" : "w-(--sidebar-width)",
      )}
    >
      {/* The head is 96px whether the sidebar is open or a rail, so the rows underneath it keep their
          place on the screen when it changes: a person who collapses the sidebar keeps their grip on
          where each page lives. The way to change the shape stands in the head beside the mark, because
          that is where it is looked for — moving it to the foot of a list that scrolls took it out of
          sight of the thing it changes. In the rail the two stack inside the same 96px: the mark, the
          control and the room between them take 68px of it, which is what leaves the mark standing on
          ground of its own rather than an inch under the top edge. The connection's dot rides the
          mark's top corner rather than its bottom one, so it never sits on the control below. */}
      <div
        className={cn(
          "flex h-24 shrink-0 items-center gap-2 px-4",
          collapsed && "flex-col justify-center gap-1 px-2",
        )}
      >
        {collapsed ? (
          // The rail has no line of words under the name, so the connection sits on the mark as a dot.
          <Tooltip content={`Moebius. ${state.label}`} side="right">
            <span className="relative flex shrink-0 items-center" data-testid="connection">
              <Mark className="size-6 text-text" />
              <StatusDot color={state.color} className="absolute -top-1 -right-1" />
              <span className="sr-only">{state.label}</span>
            </span>
          </Tooltip>
        ) : (
          <div className="flex min-w-0 flex-1 items-center gap-3">
            <Mark className="size-7 shrink-0 text-text" />
            <div className="min-w-0">
              <div className="truncate text-lg font-semibold tracking-tight text-text">Moebius</div>
              <div
                className="flex items-center gap-2 text-xs text-secondary"
                data-testid="connection"
              >
                <StatusDot color={state.color} />
                {state.label}
              </div>
            </div>
          </div>
        )}
        <Tooltip content={collapsed ? "Expand the sidebar" : "Collapse the sidebar"} side="right">
          <IconButton
            icon={SidebarSimpleIcon}
            aria-label={collapsed ? "Expand the sidebar" : "Collapse the sidebar"}
            onClick={onToggle}
          />
        </Tooltip>
      </div>
      <div className="px-2 pb-2">
        <button
          type="button"
          onClick={onSearch}
          aria-label="Search"
          className={cn(
            "flex min-h-10 w-full cursor-pointer items-center gap-2 rounded-control bg-sunken px-3 text-base text-secondary",
            "transition-colors duration-(--motion-routine) ease-out hover:bg-hover hover:text-text",
            collapsed && "justify-center px-0",
          )}
        >
          <MagnifyingGlassIcon className="size-5 shrink-0" weight="bold" />
          {!collapsed && (
            <>
              <span className="flex-1 text-left">Search</span>
              <kbd className="font-mono text-xs text-secondary">{modifier} K</kbd>
            </>
          )}
        </button>
      </div>
      {/* Back is a row of the list it leads into, so it stands under Search at the distance every first row of the
          everyday list does: the padding under Search plus its own. */}
      {inSettings && (
        <div className="px-2 py-2">
          <Back collapsed={collapsed} to={lastPage} />
        </div>
      )}
      {/* The list scrolls on its own. The rail is 56 wide, and a scrollbar there would take 8 of them and pull every
          icon off the middle of the rail, so the rail hides its scrollbar and lets the fade say what is still below;
          the wide sidebar keeps its, because its rows fill whatever width is left. */}
      <ScrollFades className={cn("min-h-0 flex-1 px-2 pb-2", collapsed && "scroll-none")}>
        {inSettings && (
          <Section title="Settings" collapsed={collapsed}>
            {SETTINGS_PAGES.map((page) => (
              <Entry
                key={page.path}
                to={`/${page.path}`}
                icon={page.icon}
                label={settingsLabel(page)}
                collapsed={collapsed}
                current={here === page.path}
              />
            ))}
          </Section>
        )}
        {!inSettings && groupSection("")}
        {!inSettings && collapsibleSection("Automation")}
        {!inSettings && (
          <Section
            title="Projects"
            collapsed={collapsed}
            collapsible={{
              // The group's own section builds its rows from the snapshot rather than from PAGES, so the folded
              // state is taken from the group here and the rows under it are the boards.
              open: projectsOpen,
              onToggle: () => toggleGroup("group.projects", !projectsOpen),
            }}
          >
            {/* The label above is not a way to get anywhere; the whole list of repositories is a row of its own, at
                the height of the boards under it. In the rail the group never folds, so the rows keep standing
                whatever it was left as. */}
            {(projectsOpen || collapsed) && (
              <>
                <Entry
                  to="/projects"
                  icon={SquaresFourIcon}
                  label="All projects"
                  collapsed={collapsed}
                  current={here === "projects"}
                />
                {boards.map((board) => {
                  const live = liveRuns(board);
                  return (
                    <Entry
                      key={board.repository.id}
                      to={boardPath(board.repository.id)}
                      icon={FolderIcon}
                      mark={{
                        color: live > 0 ? "var(--progress)" : "var(--hairline-strong)",
                        word: live > 0 ? `${live}` : "Idle",
                        tone: "progress",
                        printed: live > 0,
                      }}
                      label={board.repository.name}
                      collapsed={collapsed}
                      // The script of a column is a page of the same project, so it keeps its row marked.
                      current={
                        at.startsWith(`${boardPath(board.repository.id)}/`) ||
                        at === boardPath(board.repository.id)
                      }
                    />
                  );
                })}
              </>
            )}
          </Section>
        )}
        {!inSettings && collapsibleSection("Improve")}
        {!inSettings && !collapsed && (
          <ChatSection chats={chatList.data ?? []} snapshot={data.data} at={at} />
        )}
        {!inSettings && groupSection("Setup", "Setup")}
        {!inSettings && (
          <Section collapsed={collapsed}>
            <Entry
              to="/settings"
              icon={GearIcon}
              label="Settings"
              collapsed={collapsed}
              current={here === "settings"}
            />
          </Section>
        )}
        {inSettings && (
          <Section collapsed={collapsed}>
            <SignOut collapsed={collapsed} />
          </Section>
        )}
      </ScrollFades>
    </nav>
  );
}

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
function ChatSection({
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
  const choose = (next: ChatFilter) => {
    setFilter(next);
    writeText(CHAT_FILTER_KEY, formatChatFilter(next));
  };
  const shown = useMemo(
    () => filterChats(chats, snapshot, filter, query),
    [chats, snapshot, filter, query],
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
          "flex min-h-10 items-center gap-2 rounded-control px-3 text-base",
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

/**
 * One group of the sidebar's list. Its label and the space around it say where it starts and ends; there is no
 * rule under it, because the rail has no label to draw and a line there would divide nothing. The label names the
 * group and goes nowhere — a way in is a row, at the height of the rows beside it.
 */
function Section({
  title,
  collapsed,
  collapsible,
  children,
}: {
  title?: string;
  collapsed: boolean;
  /** Set when the group folds: the caret beside its name is the toggle, and the open state is the browser's. */
  collapsible?: { open: boolean; onToggle: () => void };
  children: React.ReactNode;
}) {
  return (
    <div className="py-2">
      {title &&
        (collapsed ? (
          // The rail has no room for the group's name, but it keeps the room its line takes, so the rows under it do
          // not climb when the sidebar narrows. The rail never folds a group: every row stays where it always was.
          <div className="mb-2 h-4 px-2" aria-hidden="true" />
        ) : collapsible ? (
          <div className="mb-2 flex h-4 items-center gap-1 px-2 text-xs font-semibold text-sidebar-group">
            {/* The label names the group and goes nowhere; the caret alone opens and closes it. */}
            {title}
            <button
              type="button"
              onClick={collapsible.onToggle}
              aria-expanded={collapsible.open}
              aria-label={`${collapsible.open ? "Collapse" : "Expand"} ${title}`}
              className="cursor-pointer text-sidebar-group"
            >
              <CaretDownIcon
                className={cn(
                  "size-3 shrink-0 transition-transform duration-(--motion-routine)",
                  !collapsible.open && "-rotate-90",
                )}
                weight="bold"
              />
            </button>
          </div>
        ) : (
          <div className="mb-2 flex h-4 items-center px-2 text-xs font-semibold text-sidebar-group">
            {title}
          </div>
        ))}
      {/* biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with
          no marker from the accessibility tree entirely */}
      <ul role="list" className="flex flex-col gap-1">
        {children}
      </ul>
    </div>
  );
}

/**
 * The way out of the settings list, and the only row above the list itself. It is a row rather than the mark at the
 * top of the sidebar because it is the same kind of thing as the rows under it: it goes somewhere, and it is found by
 * the same scan of the list that finds everything else.
 */
function Back({ collapsed, to }: { collapsed: boolean; to: string }) {
  const name = "Back to the pages";
  const link = (
    <Link
      to={to}
      aria-label={collapsed ? name : undefined}
      className={cn(
        "flex min-h-10 items-center gap-2 rounded-control px-3 text-base text-sidebar-text",
        "transition-colors duration-(--motion-routine) ease-out hover:bg-hover hover:text-text",
        collapsed && "justify-center px-0",
      )}
    >
      <ArrowLeftIcon className="size-5 shrink-0" weight="regular" />
      {!collapsed && <span className="min-w-0 flex-1 truncate">Back</span>}
    </Link>
  );
  return collapsed ? (
    <Tooltip content={name} side="right">
      {link}
    </Tooltip>
  ) : (
    link
  );
}

/**
 * The way out of the session. It is an item of the Settings list rather than a page of its own: it leaves nothing
 * behind to look at, so there is nothing a page could say that the row does not.
 */
function SignOut({ collapsed }: { collapsed: boolean }) {
  const queryClient = useQueryClient();
  const [leaving, setLeaving] = useState(false);
  const name = leaving ? "Signing out" : "Sign out";
  const row = (
    <button
      type="button"
      aria-label={collapsed ? name : undefined}
      disabled={leaving}
      className={cn(
        "flex min-h-10 w-full cursor-pointer items-center gap-2 rounded-control px-3 text-base text-sidebar-text",
        "transition-colors duration-(--motion-routine) ease-out hover:bg-hover hover:text-text",
        "disabled:cursor-default disabled:opacity-45",
        collapsed && "justify-center px-0",
      )}
      onClick={() => {
        setLeaving(true);
        void leaveSession(queryClient);
      }}
    >
      <SignOutIcon className="size-5 shrink-0" weight="regular" />
      {!collapsed && <span className="min-w-0 flex-1 truncate text-left">{name}</span>}
    </button>
  );
  // The row's own words are enough while they are on screen; only the rail has to say them again.
  return (
    <li>
      {collapsed ? (
        <Tooltip content={name} side="right">
          {row}
        </Tooltip>
      ) : (
        row
      )}
    </li>
  );
}

function Entry({
  to,
  icon: IconComponent,
  mark,
  label,
  collapsed,
  trailing,
  current,
}: {
  to: string;
  icon?: Icon;
  /** The state of what the row leads to, at the end of the row where its words used to stand. */
  mark?: RowState;
  label: string;
  collapsed: boolean;
  trailing?: Trailing;
  /** Whether this entry leads to what is on screen now. */
  current: boolean;
}) {
  // The rail has no room for the words after a name, so the link takes them into its own name and the tooltip repeats
  // them: the state of a board is never only the colour of its dot.
  const quiet = mark?.word ?? trailing?.text;
  const name = collapsed && quiet ? `${label}. ${quiet}` : label;
  // A row whose state is a mark has the word nowhere it can be read as a word, so the name of the link takes it —
  // the same words the rail is given, which is also how a name stays one name whatever the shape of the sidebar.
  const spoken = collapsed || !mark ? undefined : `${label}. ${mark.word}`;
  const rows = (
    <>
      {IconComponent && <IconComponent className="size-5 shrink-0" weight="regular" />}
      {/* The rail has no end to a row to put a mark at, so there it rides beside the icon; in the list it stands
          where the words it replaces stood. */}
      {collapsed && mark && <StatusDot color={mark.color} />}
      {!collapsed && (
        <>
          <span className="min-w-0 flex-1 truncate" title={label}>
            {label}
          </span>
          {trailing && (
            <span className={cn("shrink-0 text-sm tabular-nums", TRAILING[trailing.tone])}>
              {trailing.text}
            </span>
          )}
          {mark && (
            <>
              {mark.printed && (
                <span className={cn("shrink-0 text-sm tabular-nums", TRAILING[mark.tone])}>
                  {mark.word}
                </span>
              )}
              <StatusDot color={mark.color} />
            </>
          )}
        </>
      )}
    </>
  );
  const shape = cn(
    "flex min-h-10 items-center gap-2 rounded-control px-3 text-base",
    collapsed && "justify-center px-0",
  );
  // The entry that stands wears the same fill as the chosen row of a page's list, so one fill means one thing
  // wherever a thing is chosen.
  const standing = "bg-selected font-semibold text-text";
  const waiting = "text-sidebar-text hover:bg-hover hover:text-text";
  // Which entry stands is worked out here rather than asked of the router in a callback, because the rail wraps the
  // row in a tooltip and a tooltip's trigger folds a function `className` into text — the row would keep its link and
  // lose its whole shape, which is what made the rail's icons sit uncentred at half height.
  const link = (
    <Link
      to={to}
      aria-label={collapsed ? name : spoken}
      aria-current={current ? "page" : undefined}
      className={cn(shape, current ? standing : waiting)}
    >
      {rows}
    </Link>
  );
  return (
    <li>
      {collapsed ? (
        <Tooltip content={name} side="right">
          {link}
        </Tooltip>
      ) : (
        link
      )}
    </li>
  );
}
