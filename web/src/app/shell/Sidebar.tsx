/**
 * The sidebar on every page, as the standard draws it (`docs/design-web.md`, Sidebar): 224px of quiet rows on the
 * canvas. At the top the search button (the server switch of a later issue stands beside it); then Inbox with the
 * count of what needs you, My issues and Fleet, which says "n running" beside a green dot only while anything runs;
 * Projects — its name leads to the project list, a row for each project leads to its board; Workspace with Wiki,
 * Cycles, Views and More; and at the foot Settings, alone. The sidebar never changes its contents: on a Settings page
 * the same list stands, with Settings chosen. The counts come from the snapshot. The sidebar also notes the places a
 * person opens (a project's board, a card on it) for the server to keep.
 */

import { HashIcon } from "@phosphor-icons/react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import { useLocation } from "react-router";
import { snapshot } from "../../api/calls/board";
import { notices as readNotices } from "../../api/calls/notices";
import { seen } from "../../api/calls/places";
import { keys } from "../../api/keys";
import { liveRuns } from "../../api/types/board";
import { IconButton } from "../../ui/components/IconButton";
import { SearchIcon } from "../../ui/icons";
import { waiting } from "../inbox/notices";
import { boardPath, isSettingsPage, PAGES, type PageDef, pageByPath, shownTo } from "../pages";
import { inSettingsArea } from "../settings/settingsEntries";
import { placeAt } from "./places";
import { Entry, type Trailing } from "./SidebarEntry";
import { SettingsFoot } from "./SidebarFoot";
import { Section } from "./SidebarSection";
import { useRole } from "./session";

interface SidebarProps {
  onSearch: () => void;
}

export function Sidebar({ onSearch }: SidebarProps) {
  const role = useRole();
  const data = useQuery({ queryKey: keys.snapshot, queryFn: () => snapshot() });
  const boards = data.data?.boards ?? [];
  const location = useLocation();
  const at = location.pathname;
  const here = at.replace(/^\/+|\/+$/g, "").split("/")[0] ?? "";
  // What waits for you: the items of the inbox and the unread notices given to you or naming you.
  const notices = useQuery({ queryKey: keys.notices, queryFn: readNotices });
  const inbox = waiting(data.data?.inbox.length ?? 0, notices.data ?? []);
  const working = boards.reduce((sum, board) => sum + liveRuns(board), 0);

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
  // Not when the first snapshot arrives: the lists were read beside it, and reading them again would ask twice.
  const projects = boards.length;
  const knownProjects = useRef<number | null>(null);
  useEffect(() => {
    if (!loaded) return;
    if (knownProjects.current !== null && knownProjects.current !== projects)
      void queryClient.invalidateQueries({ queryKey: keys.places });
    knownProjects.current = projects;
  }, [projects, loaded, queryClient]);

  // What waits for you is counted even when the answer is none; what the machines are doing is said only when
  // something is.
  const trailingOf = (path: string): Trailing | undefined => {
    if (path === "inbox") return { text: `${inbox}`, tone: "count" };
    if (path === "fleet" && working > 0) return { text: `${working} running`, tone: "running" };
    return undefined;
  };

  const entryOf = (page: PageDef) => (
    <Entry
      key={page.path}
      to={`/${page.path}`}
      current={here === page.path}
      icon={page.icon}
      label={page.title}
      trailing={trailingOf(page.path)}
    />
  );
  const pagesOf = (group: PageDef["group"]) =>
    PAGES.filter((page) => page.group === group && shownTo(page, role));
  const projectList = pageByPath("projects");

  return (
    <nav aria-label="Pages" className="flex h-full w-(--sidebar-width) shrink-0 flex-col">
      <div className="flex h-12 shrink-0 items-center justify-end px-2">
        <IconButton
          icon={SearchIcon}
          iconWeight="regular"
          iconClassName="size-5"
          aria-label="Search"
          title="Search"
          onClick={onSearch}
        />
      </div>
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-2 pb-2">
        <Section>{pagesOf("").map(entryOf)}</Section>
        <Section title="Projects" to={projectList ? `/${projectList.path}` : undefined}>
          {boards.map((board) => {
            const to = boardPath(board.repository.id);
            return (
              <Entry
                key={board.repository.id}
                to={to}
                icon={HashIcon}
                label={board.repository.name}
                current={at === to || at.startsWith(`${to}/`)}
              />
            );
          })}
        </Section>
        <Section title="Workspace">
          {pagesOf("Workspace").map(entryOf)}
          {pagesOf("More").map(entryOf)}
        </Section>
        <div className="mt-auto pt-4">
          <SettingsFoot current={inSettingsArea(at) || isSettingsPage(here)} />
        </div>
      </div>
    </nav>
  );
}
