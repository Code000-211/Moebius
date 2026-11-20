/**
 * The command palette, opened with Ctrl+K (Cmd+K on a Mac) or the sidebar's Search entry. It lists, in this order: the
 * commands of what is in focus (a card open in the dock) with their keys, the places seen lately, and then what a
 * query searches by kind — cards, the wiki's pages, pages and places (the projects). A query keeps the commands of the focus while they
 * match and finds the rest by name or by kind (`card`, `page`, `place`). Arrows, Enter and Escape are the primitives'
 * (`ui/components/Command.tsx`, from cmdk); closing hands the focus back to where it was, unless the command opened a
 * dialog that takes it. The three places the feature is named — the button, the dialog, the field — say Search.
 */

import { BookOpenTextIcon, FolderIcon, KanbanIcon, UserIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router";
import { snapshot } from "../../api/calls/board";
import { people as loadPeople } from "../../api/calls/people";
import { searchWiki } from "../../api/calls/wiki";
import { keys } from "../../api/keys";
import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "../../ui/components/Command";
import { useOpenCard } from "../board/CardWindowHost";
import { cardKey } from "../board/cardKey";
import { useNewIssue } from "../board/NewIssueHost";
import { usePause } from "../fleet/usePause";
import { boardPath, type Group, PAGES, shownTo } from "../pages";
import { usePaletteFocus } from "./palette/focus";
import { matches, type PaletteCommand, ranked, readRecent } from "./palette/model";
import { NEW_ISSUE_KEY, SHORTCUTS_KEY } from "./palette/shortcuts";
import { useRole } from "./session";

const GROUPS: readonly Group[] = ["", "Projects", "Plan", "Build", "Improve", "Setup"];
/** Cards shown for a query: the list is for choosing, and a longer one is a reason to type more. */
const CARDS_SHOWN = 8;
/** Wiki pages shown for a query, and how long typing pauses before the wiki is searched. */
const WIKI_SHOWN = 6;
const PEOPLE_SHOWN = 5;
const WIKI_PAUSE_MS = 200;

/** `value` as it was `ms` milliseconds ago, so a search waits for a person to pause. */
function useDebounced<T>(value: T, ms: number): T {
  const [later, setLater] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setLater(value), ms);
    return () => clearTimeout(timer);
  }, [value, ms]);
  return later;
}

/** A key of a command, drawn as the small chip beside it. */
function KeyHint({ children }: { children: string }) {
  return (
    <kbd className="ml-auto rounded-small bg-hover px-1.5 font-sans text-xs uppercase text-secondary">
      {children}
    </kbd>
  );
}

export function CommandBar({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const navigate = useNavigate();
  const role = useRole();
  const here = useLocation().pathname;
  const focus = usePaletteFocus();
  const newIssue = useNewIssue();
  const openCard = useOpenCard();
  const pause = usePause(open);
  const data = useQuery({ queryKey: keys.snapshot, queryFn: () => snapshot(), enabled: open });
  const [query, setQuery] = useState("");
  // Set by a command that opens a dialog: that dialog takes the focus, and handing it back here would steal it.
  const keepFocus = useRef(false);

  // The wiki's pages found by the words typed, once typing pauses; a server with no wiki finds none.
  const typed = useDebounced(query.trim(), WIKI_PAUSE_MS);
  const found = useQuery({
    queryKey: keys.wikiSearch(typed),
    queryFn: () => searchWiki(typed, undefined, WIKI_SHOWN),
    enabled: open && typed !== "",
    retry: false,
  });
  // Searching: the words are typed and the wiki has not answered yet, so "Nothing matches" would be a guess.
  const searching = query.trim() !== "" && (typed !== query.trim() || found.isFetching);
  const wikiHits = typed !== "" && Array.isArray(found.data) ? found.data : [];

  const boards = data.data?.boards ?? [];
  const everyone = useQuery({
    queryKey: keys.people,
    queryFn: loadPeople,
    enabled: open && role === "owner",
  });
  const recent = useMemo(() => (open ? readRecent() : []), [open]);

  function close() {
    setQuery("");
    onOpenChange(false);
  }
  function go(path: string) {
    close();
    void navigate(path);
  }
  function run(command: PaletteCommand) {
    keepFocus.current = command.opensDialog === true;
    close();
    command.run();
  }

  // The commands that work wherever a person is, with their keys: the same keys the shortcuts page lists.
  const everywhere: (PaletteCommand & { words?: string })[] = [
    {
      id: "new-issue",
      label: "New issue",
      key: NEW_ISSUE_KEY,
      opensDialog: true,
      run: () => newIssue(),
    },
    {
      id: "shortcuts",
      label: "Keyboard shortcuts",
      key: SHORTCUTS_KEY,
      run: () => void navigate("/shortcuts"),
    },
    pause.paused
      ? {
          id: "resume-all",
          label: "Resume all agents",
          words: "start continue unpause",
          run: pause.toggle,
        }
      : {
          id: "pause-all",
          label: "Pause all agents",
          words: "stop hold halt",
          run: pause.toggle,
        },
    {
      id: "stop-runs",
      label: "Stop a run",
      words: "pause halt kill agent fleet",
      run: () => void navigate("/fleet"),
    },
  ].filter((c) => matches(query, `${c.label} command ${c.words ?? ""}`));
  const commands = (focus?.commands ?? []).filter((c) =>
    matches(query, `${c.label} command ${focus?.label ?? ""}`),
  );
  const pages = ranked(
    query,
    PAGES.filter((page) => shownTo(page, role)),
    (page) => `${page.title} page ${page.group}`,
  );
  const places = ranked(query, boards, (board) => `${board.repository.name} place project`);
  const cards =
    query.trim() === ""
      ? []
      : ranked(
          query,
          boards.flatMap((board) => board.cards.map((card) => ({ card, board }))),
          ({ card }) => `${cardKey(card)} #${card.number} ${card.title} card`,
        ).slice(0, CARDS_SHOWN);
  // People are found by name or handle; the People page is the owner's, so it is the owner who is offered them.
  const folk =
    query.trim() === "" || role !== "owner"
      ? []
      : ranked(
          query,
          (Array.isArray(everyone.data) ? everyone.data : []).filter((person) => !person.disabled),
          (person) => `${person.name} ${person.handle} person`,
        ).slice(0, PEOPLE_SHOWN);

  // What was seen lately, by name, where the name is still known; the place a person is at is not offered to go to.
  const known = new Map<string, { label: string; icon: ReactNode }>();
  for (const page of PAGES)
    known.set(`/${page.path}`, {
      label: page.title,
      icon: <page.icon className="size-5" weight="regular" />,
    });
  for (const board of boards)
    known.set(boardPath(board.repository.id), {
      label: board.repository.name,
      icon: <FolderIcon className="size-5" weight="regular" />,
    });
  const lately =
    query.trim() === ""
      ? recent.flatMap((path) => {
          const place = known.get(path);
          return place && path !== here ? [{ path, ...place }] : [];
        })
      : [];

  return (
    <CommandDialog
      open={open}
      onOpenChange={(next) => {
        if (!next) setQuery("");
        onOpenChange(next);
      }}
      onCloseAutoFocus={(event) => {
        if (keepFocus.current) event.preventDefault();
        keepFocus.current = false;
      }}
    >
      <Command label="Search" shouldFilter={false}>
        <CommandInput placeholder="Search…" value={query} onValueChange={setQuery} />
        <CommandList label="Commands and places">
          <CommandEmpty>{searching ? "Searching…" : "Nothing matches."}</CommandEmpty>
          {commands.length > 0 && focus && (
            <CommandGroup heading={focus.label}>
              {commands.map((command) => (
                <CommandItem
                  key={command.id}
                  value={`command ${command.id}`}
                  onSelect={() => run(command)}
                >
                  <span>{command.label}</span>
                  {command.key && <KeyHint>{command.key}</KeyHint>}
                </CommandItem>
              ))}
            </CommandGroup>
          )}
          {everywhere.length > 0 && (
            <CommandGroup heading="Commands">
              {everywhere.map((command) => (
                <CommandItem
                  key={command.id}
                  value={`command ${command.id}`}
                  onSelect={() => run(command)}
                >
                  <span>{command.label}</span>
                  {command.key && <KeyHint>{command.key}</KeyHint>}
                </CommandItem>
              ))}
            </CommandGroup>
          )}
          {lately.length > 0 && (
            <CommandGroup heading="Recent">
              {lately.map((place) => (
                <CommandItem
                  key={place.path}
                  value={`recent ${place.path}`}
                  onSelect={() => go(place.path)}
                >
                  {place.icon}
                  <span>{place.label}</span>
                </CommandItem>
              ))}
            </CommandGroup>
          )}
          {cards.length > 0 && (
            <CommandGroup heading="Cards">
              {cards.map(({ card, board }) => (
                <CommandItem
                  key={card.id}
                  value={`card ${card.id}`}
                  onSelect={() => {
                    close();
                    openCard(card.id);
                  }}
                >
                  <KanbanIcon className="size-5" weight="regular" />
                  <span className="truncate">{`${cardKey(card)} ${card.title}`}</span>
                  <span className="ml-auto shrink-0 text-xs text-muted">
                    {board.repository.name}
                  </span>
                </CommandItem>
              ))}
            </CommandGroup>
          )}
          {folk.length > 0 && (
            <CommandGroup heading="People">
              {folk.map((person) => (
                <CommandItem
                  key={person.id}
                  value={`person ${person.id}`}
                  onSelect={() => go("/people")}
                >
                  <UserIcon className="size-5" weight="regular" />
                  <span className="truncate">{person.name}</span>
                  <span className="ml-auto shrink-0 truncate text-xs text-muted">
                    @{person.handle}
                  </span>
                </CommandItem>
              ))}
            </CommandGroup>
          )}
          {wikiHits.length > 0 && (
            <CommandGroup heading="Wiki">
              {wikiHits.map((hit) => (
                <CommandItem
                  key={hit.path}
                  value={`wiki ${hit.path}`}
                  onSelect={() => go(`/wiki?page=${encodeURIComponent(hit.path)}`)}
                >
                  <BookOpenTextIcon className="size-5" weight="regular" />
                  <span className="truncate">{hit.name}</span>
                  <span className="ml-auto shrink-0 truncate text-xs text-muted">
                    {hit.project ?? "Organisation"}
                  </span>
                </CommandItem>
              ))}
            </CommandGroup>
          )}
          {GROUPS.map((group) => {
            const inGroup = pages.filter((page) => page.group === group);
            const heading = group || "Pages";
            const boardsHere = group === "Projects" ? places : [];
            if (inGroup.length + boardsHere.length === 0) return null;
            return (
              <CommandGroup key={heading} heading={heading}>
                {inGroup.map((page) => (
                  <CommandItem
                    key={page.path}
                    value={`page ${page.path}`}
                    onSelect={() => go(`/${page.path}`)}
                  >
                    <page.icon className="size-5" weight="regular" />
                    <span>{page.title}</span>
                  </CommandItem>
                ))}
                {boardsHere.map((board) => (
                  <CommandItem
                    key={board.repository.id}
                    value={`place ${board.repository.id}`}
                    onSelect={() => go(boardPath(board.repository.id))}
                  >
                    <FolderIcon className="size-5" weight="regular" />
                    <span>{board.repository.name}</span>
                  </CommandItem>
                ))}
              </CommandGroup>
            );
          })}
        </CommandList>
      </Command>
    </CommandDialog>
  );
}
