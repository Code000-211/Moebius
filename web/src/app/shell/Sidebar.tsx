/**
 * The sidebar on every page, in the order of the drafts: the organisation (the app name with the connection) as a
 * slightly raised surface, the search (the command bar), Inbox and Fleet, the project switcher with the chosen
 * project's places under it (its Board and its Files), Automate (Automations, Workflows and Improve as one row that
 * folds over Retro, Experiments and Decisions), the list of chats below them in the wide sidebar, then Setup and, at
 * the foot, the gear to Settings. The chosen row is a slightly raised surface too, and the names of the groups are in
 * small capitals. Places that come with later issues are added by them. The gear is — one gear alone, drawn as the rail draws its rows
 * and standing under the settings list's view as well, below the row that leaves the session. It collapses
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

import { MagnifyingGlassIcon, SidebarSimpleIcon } from "@phosphor-icons/react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router";
import { snapshot } from "../../api/calls/board";
import { chats as readChats } from "../../api/calls/chat";
import { notices as readNotices } from "../../api/calls/notices";
import { seen } from "../../api/calls/places";
import { start } from "../../api/calls/server";
import type { ConnectionState } from "../../api/events";
import { keys } from "../../api/keys";
import { liveRuns } from "../../api/types/board";
import { recentChat } from "../../chat/chatList";
import { unreadCount } from "../../chat/readMarkers";
import { IconButton } from "../../ui/components/IconButton";
import { Mark } from "../../ui/components/Mark";
import { StatusDot } from "../../ui/components/StatusDot";
import { Tooltip } from "../../ui/components/Tooltip";
import { cn } from "../../ui/lib/cn";
import { getModifierKey } from "../../ui/lib/platform";
import { ScrollFades } from "../../ui/page/ScrollFades";
import { waiting } from "../inbox/notices";
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
  shownTo,
} from "../pages";
import { readFlagOr, readText, writeFlag, writeText } from "../storage";
import { ChatSection } from "./ChatSection";
import { PlacesSection } from "./PlacesSection";
import { ProjectSection } from "./ProjectSection";
import { placeAt } from "./places";
import { Entry, type Trailing } from "./SidebarEntry";
import { Back, SettingsFoot, SignedInAs, SignOut } from "./SidebarFoot";
import { ImproveRow, Section } from "./SidebarSection";
import { useRole } from "./session";

const STATE: Record<ConnectionState, { label: string; color: string }> = {
  online: { label: "Online", color: "var(--complete)" },
  connecting: { label: "Connecting", color: "var(--text-secondary)" },
  reconnecting: { label: "Reconnecting", color: "var(--attention)" },
};

/** Where the chosen project of the switcher is kept for this browser. */
const PROJECT_KEY = "moebius.sidebar.project";

/** Where Improve's open or closed state is kept for this browser; a browser that never chose has it open. */
const IMPROVE_KEY = "moebius.sidebar.group.improve";

/** The places Improve holds, in the order the sidebar lists them. In the rail it does not fold. */
const IMPROVE_PATHS: readonly string[] = ["retro", "experiments", "decision-model"];

/** The places of Automate besides Improve. */
const AUTOMATE_PATHS: readonly string[] = ["automations", "workflows"];

/** The project a page of a project belongs to (`/board/3`, `/projects/3`), when the address names one. */
function projectOf(at: string): number | null {
  const match = /^\/(?:board|projects)\/(\d+)(?:\/|$)/.exec(at);
  return match ? Number(match[1]) : null;
}

interface SidebarProps {
  connection: ConnectionState;
  collapsed: boolean;
  onToggle: () => void;
  onSearch: () => void;
}

export function Sidebar({ connection, collapsed, onToggle, onSearch }: SidebarProps) {
  const role = useRole();
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
  // What waits for you: the items of the inbox and the unread notices given to you or naming you.
  const notices = useQuery({ queryKey: keys.notices, queryFn: readNotices });
  const inbox = waiting(data.data?.inbox.length ?? 0, notices.data ?? []);
  const working = boards.reduce((sum, board) => sum + liveRuns(board), 0);
  const state = STATE[connection];
  const modifier = getModifierKey();
  // Whether Improve is open. A browser that has never chosen anything has it open.
  const [improveOpen, setImproveOpen] = useState(() => readFlagOr(IMPROVE_KEY, true));
  const toggleImprove = (open: boolean) => {
    setImproveOpen(open);
    writeFlag(IMPROVE_KEY, open);
  };
  // The chosen project: the one the address names, else the one this browser kept, else the first. What the address
  // names is kept, so the project stays chosen on every other page and after a reload.
  const navigate = useNavigate();
  const [kept, setKept] = useState<number | null>(() => {
    const text = readText(PROJECT_KEY);
    return text === null || text === "" ? null : Number(text);
  });
  const known = (id: number | null): id is number =>
    id !== null && boards.some((board) => board.repository.id === id);
  const named = projectOf(at);
  const chosen = known(named) ? named : known(kept) ? kept : (boards[0]?.repository.id ?? null);
  useEffect(() => {
    if (known(named) && named !== kept) {
      setKept(named);
      writeText(PROJECT_KEY, String(named));
    }
  });
  const choose = (id: number) => {
    setKept(id);
    writeText(PROJECT_KEY, String(id));
    navigate(boardPath(id));
  };

  // What a person opens is kept for them on the server: a project's board or a card open on it. A place is noted
  // once for each time the address changes to it, and the lists are read again, so a pin or a place the person can
  // no longer see is up to date.
  const queryClient = useQueryClient();
  const { pathname, search } = location;
  // A place is noted once it loaded: the snapshot names its project. One that is not there (a private project of
  // another person, a removed one) is no place, and asking the server to note it would only answer 404.
  const noted = useRef("");
  const loaded = data.data !== undefined;
  useEffect(() => {
    const place = placeAt(pathname, search);
    if (!place || !loaded) return;
    const project = Number(/^\/(?:board|projects)\/(\d+)/.exec(pathname)?.[1]);
    const address = `${pathname}${search}`;
    if (noted.current === address || !boards.some((board) => board.repository.id === project))
      return;
    noted.current = address;
    seen(place)
      .then((next) => queryClient.setQueryData(keys.places, next))
      .catch(() => {});
  }, [pathname, search, queryClient, loaded, boards]);
  // A project that was removed takes its places with it, so the lists are read again when the projects change.
  const projects = boards.length;
  useEffect(() => {
    if (projects >= 0) void queryClient.invalidateQueries({ queryKey: keys.places });
  }, [projects, queryClient]);

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
    // The pages Automate holds are its section's to show, not the flat one's.
    const held = new Set([...AUTOMATE_PATHS, ...IMPROVE_PATHS]);
    const pages = pagesOf(group).filter((page) => !held.has(page.path));
    if (pages.length === 0) return null;
    return (
      <Section title={title} collapsed={collapsed}>
        {pages.map(entryOf)}
      </Section>
    );
  };

  return (
    <nav
      aria-label="Pages"
      className={cn(
        "flex h-full shrink-0 flex-col",
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
          <div className="flex min-w-0 flex-1 items-center gap-3 rounded-control bg-raised px-3 py-2 shadow-card">
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
            "flex min-h-8 w-full cursor-pointer items-center gap-2 rounded-control bg-sunken px-3 text-base text-secondary",
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
            {SETTINGS_PAGES.filter((page) => shownTo(page, role)).map((page) => (
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
        {!inSettings && (
          <ProjectSection
            boards={boards}
            chosen={chosen}
            collapsed={collapsed}
            at={at}
            here={here}
            onChoose={choose}
          />
        )}
        {!inSettings && groupSection("Plan", "Plan")}
        {!inSettings && !collapsed && <PlacesSection at={at} />}
        {!inSettings && (
          <Section title="Automate" collapsed={collapsed}>
            {AUTOMATE_PATHS.map(pageByPath)
              .filter((page): page is PageDef => page !== undefined)
              .map(entryOf)}
            <ImproveRow
              collapsed={collapsed}
              open={improveOpen}
              onToggle={() => toggleImprove(!improveOpen)}
            >
              {IMPROVE_PATHS.map(pageByPath)
                .filter((page): page is PageDef => page !== undefined)
                .map(entryOf)}
            </ImproveRow>
          </Section>
        )}
        {!inSettings && !collapsed && (
          <ChatSection chats={chatList.data ?? []} snapshot={data.data} at={at} />
        )}
        {!inSettings && groupSection("Setup", "Setup")}
        {inSettings && (
          <Section collapsed={collapsed}>
            {!collapsed && <SignedInAs />}
            <SignOut collapsed={collapsed} />
          </Section>
        )}
        {/* The one row the list never exchanges: the gear to Settings is the foot of it on every page, drawn as the
            rail draws its rows, so a hand that learned its place keeps it while the list above it is swapped for the
            settings'. */}
        <Section collapsed={collapsed}>
          <SettingsFoot />
        </Section>
      </ScrollFades>
    </nav>
  );
}
