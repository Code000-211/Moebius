/**
 * The command palette, opened with Ctrl+K (Cmd+K on a Mac) or the sidebar's Search entry. It lists, in this order: the
 * commands of what is in focus (a card open in the dock) with their keys, the places seen lately, and then what a
 * query searches by kind — cards, pages and places (the projects). A query keeps the commands of the focus while they
 * match and finds the rest by name or by kind (`card`, `page`, `place`). Arrows, Enter and Escape are the primitives'
 * (`ui/components/Command.tsx`, from cmdk); closing hands the focus back to where it was, unless the command opened a
 * dialog that takes it. The three places the feature is named — the button, the dialog, the field — say Search.
 */

import { FolderIcon, KanbanIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { type ReactNode, useMemo, useRef, useState } from "react";
import { useLocation, useNavigate } from "react-router";
import { snapshot } from "../../api/calls/board";
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
import { cardKey } from "../board/cardKey";
import { useNewIssue } from "../board/NewIssueHost";
import { boardPath, type Group, PAGES, shownTo } from "../pages";
import { usePaletteFocus } from "./palette/focus";
import { matches, type PaletteCommand, readRecent } from "./palette/model";
import { NEW_ISSUE_KEY, SHORTCUTS_KEY } from "./palette/shortcuts";
import { useRole } from "./session";

const GROUPS: readonly Group[] = ["", "Projects", "Plan", "Build", "Improve", "Setup"];
/** Cards shown for a query: the list is for choosing, and a longer one is a reason to type more. */
const CARDS_SHOWN = 8;

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
  const data = useQuery({ queryKey: keys.snapshot, queryFn: () => snapshot(), enabled: open });
  const [query, setQuery] = useState("");
  // Set by a command that opens a dialog: that dialog takes the focus, and handing it back here would steal it.
  const keepFocus = useRef(false);

  const boards = data.data?.boards ?? [];
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
  const everywhere: PaletteCommand[] = [
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
  ].filter((c) => matches(query, `${c.label} command`));
  const commands = (focus?.commands ?? []).filter((c) =>
    matches(query, `${c.label} command ${focus?.label ?? ""}`),
  );
  const pages = PAGES.filter((page) => shownTo(page, role)).filter((page) =>
    matches(query, `${page.title} page ${page.group}`),
  );
  const places = boards.filter((board) => matches(query, `${board.repository.name} place project`));
  const cards =
    query.trim() === ""
      ? []
      : boards
          .flatMap((board) => board.cards.map((card) => ({ card, board })))
          .filter(({ card }) =>
            matches(query, `${cardKey(card)} #${card.number} ${card.title} card`),
          )
          .slice(0, CARDS_SHOWN);

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
          <CommandEmpty>Nothing matches.</CommandEmpty>
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
                  onSelect={() => go(`${boardPath(board.repository.id)}?card=${card.id}`)}
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
