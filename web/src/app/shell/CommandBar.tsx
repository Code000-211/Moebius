/**
 * The command palette, opened with Ctrl+K (Cmd+K on a Mac) or the sidebar's Search entry: a plain list of rows, each an
 * icon and a name, with no key hints. It lists, in this order: the commands of what is in focus (a card open in the
 * dock), the commands that work anywhere, the places seen lately, and then what a query searches by kind — cards, the
 * wiki's pages, pages and places (the projects). A query keeps the commands of the focus while they match and finds
 * the rest by name or by kind (`card`, `page`, `place`). Arrows, Enter and Escape are the primitives' (`ui/components/
 * Command.tsx`, from cmdk); Tab with text typed sends it to the agent's panel and closes the palette, and Tab with
 * nothing typed does nothing. Closing hands the focus back to where it was, unless the command opened a dialog that
 * takes it. The three places the feature is named — the button, the dialog, the field — say Search.
 */

import { useQuery } from "@tanstack/react-query";
import { type KeyboardEvent, type ReactNode, useEffect, useMemo, useRef, useState } from "react";
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
import {
  AddIcon,
  AgentIcon,
  BoardIcon,
  FolderIcon,
  KeyIcon,
  ListChecksIcon,
  PauseIcon,
  PersonIcon,
  PlayIcon,
  StopIcon,
  WikiIcon,
} from "../../ui/icons";
import { cardKey } from "../board/cardKey";
import { useNewIssue } from "../board/NewIssueHost";
import { usePause } from "../fleet/usePause";
import { boardPath, PAGES, shownTo } from "../pages";
import { useAgent } from "./AgentContext";
import { usePaletteFocus } from "./palette/focus";
import { matches, type PaletteCommand, ranked, readRecent } from "./palette/model";
import { useRole } from "./session";

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

/** What a row of the palette draws: an icon of the row's own size, then its name. */
const ICON = "size-5 shrink-0";

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
  const agent = useAgent();
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

  /** Tab with text typed hands it to the agent, which opens its panel and sends it; with none, Tab does nothing. */
  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key !== "Tab" || event.shiftKey) return;
    event.preventDefault();
    const text = query.trim();
    if (text === "") return;
    keepFocus.current = true;
    close();
    agent.ask(text);
  }

  // The commands that work wherever a person is.
  const everywhere: (PaletteCommand & { words?: string; icon: ReactNode })[] = [
    {
      id: "new-issue",
      label: "New issue",
      icon: <AddIcon className={ICON} />,
      opensDialog: true,
      run: () => newIssue(),
    },
    {
      id: "new-conversation",
      label: "New conversation",
      words: "agent ask chat moebius",
      icon: <AgentIcon className={ICON} />,
      opensDialog: true,
      run: agent.newConversation,
    },
    {
      id: "shortcuts",
      label: "Keyboard shortcuts",
      words: "keys",
      icon: <KeyIcon className={ICON} />,
      run: () => void navigate("/shortcuts"),
    },
    pause.paused
      ? {
          id: "resume-all",
          icon: <PlayIcon className={ICON} />,
          label: "Resume all agents",
          words: "start continue unpause",
          run: pause.toggle,
        }
      : {
          id: "pause-all",
          icon: <PauseIcon className={ICON} />,
          label: "Pause all agents",
          words: "stop hold halt",
          run: pause.toggle,
        },
    {
      id: "stop-runs",
      icon: <StopIcon className={ICON} />,
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
      icon: <page.icon className={ICON} weight="regular" />,
    });
  for (const board of boards)
    known.set(boardPath(board.repository.id), {
      label: board.repository.name,
      icon: <FolderIcon className={ICON} />,
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
      <Command label="Search" shouldFilter={false} onKeyDown={onKeyDown}>
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
                  <ListChecksIcon className={ICON} />
                  <span>{command.label}</span>
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
                  {command.icon}
                  <span>{command.label}</span>
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
                  onSelect={() => go(`${boardPath(board.repository.id)}?card=${card.id}`)}
                >
                  <BoardIcon className={ICON} />
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
                  <PersonIcon className={ICON} />
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
                  <WikiIcon className={ICON} />
                  <span className="truncate">{hit.name}</span>
                  <span className="ml-auto shrink-0 truncate text-xs text-muted">
                    {hit.project ?? "Organisation"}
                  </span>
                </CommandItem>
              ))}
            </CommandGroup>
          )}
          {pages.length > 0 && (
            <CommandGroup heading="Pages">
              {pages.map((page) => (
                <CommandItem
                  key={page.path}
                  value={`page ${page.path}`}
                  onSelect={() => go(`/${page.path}`)}
                >
                  <page.icon className={ICON} weight="regular" />
                  <span>{page.title}</span>
                </CommandItem>
              ))}
            </CommandGroup>
          )}
          {places.length > 0 && (
            <CommandGroup heading="Projects">
              {places.map((board) => (
                <CommandItem
                  key={board.repository.id}
                  value={`place ${board.repository.id}`}
                  onSelect={() => go(boardPath(board.repository.id))}
                >
                  <FolderIcon className={ICON} />
                  <span>{board.repository.name}</span>
                </CommandItem>
              ))}
            </CommandGroup>
          )}
        </CommandList>
      </Command>
    </CommandDialog>
  );
}
