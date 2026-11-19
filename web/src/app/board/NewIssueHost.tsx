/**
 * The form of a new issue for the whole app. The shell wraps its pages in `NewIssueProvider`, which draws the form
 * (`NewIssueDialog.tsx`) over whatever page is open and opens it on `C` — from any page, but not while a person is
 * typing in a field, not with a modifier held, and not while another sheet is over the page. A page opens it itself
 * with `useNewIssue().open(...)`: the board's "New issue" button, a card's "Add a sub-issue", a column's "+". The form
 * is for the repository the page shows (the board's or the project's), or the first one there is, and offers the
 * others as a chip. A card made from a page that has no way to say so (`onCreated` left out) is said in a line at the
 * foot of the window, which goes away by itself.
 */
import { useQuery } from "@tanstack/react-query";
import { createContext, type ReactNode, useCallback, useContext, useEffect, useState } from "react";
import { useLocation } from "react-router";
import { snapshot } from "../../api/calls/board";
import { keys } from "../../api/keys";
import type { Card } from "../../api/types/board";
import { modalIsOpen } from "../../ui/lib/shortcuts";
import { Notice } from "../../ui/page/Notice";
import { NewIssueDialog } from "./NewIssueDialog";

/** What opens the form: the repository (the page's own when left out), a parent, and who hears of the card made. */
export interface NewIssueRequest {
  repository?: number;
  parent?: number | null;
  onCreated?: (card: Card) => void;
}

type Open = (request?: NewIssueRequest) => void;

const NewIssueContext = createContext<Open>(() => {});

/** Opens the form of a new issue. Outside a provider (a page drawn alone in a test) it does nothing. */
export function useNewIssue(): Open {
  return useContext(NewIssueContext);
}

/** How long the line that says a card was made stays, in ms. */
const SAID_FOR = 5000;

/** Whether a key pressed now is a person typing, which `C` must leave alone. */
export function isTyping(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return (
    target.isContentEditable ||
    target.tagName === "INPUT" ||
    target.tagName === "TEXTAREA" ||
    target.tagName === "SELECT"
  );
}

/** The repository a path is about: `/board/3` and `/projects/3` are about 3. */
function repositoryOf(pathname: string): number | null {
  const [first, second] = pathname.split("/").filter(Boolean);
  if ((first === "board" || first === "projects") && second !== undefined) {
    const id = Number(second);
    return Number.isInteger(id) ? id : null;
  }
  return null;
}

export function NewIssueProvider({ children }: { children: ReactNode }) {
  const location = useLocation();
  const data = useQuery({ queryKey: keys.snapshot, queryFn: () => snapshot() });
  const [request, setRequest] = useState<NewIssueRequest | null>(null);
  // Bumped at every opening, so a form opened again starts afresh.
  const [opened, setOpened] = useState(0);
  const [chosen, setChosen] = useState<number | null>(null);
  const [said, setSaid] = useState<string | null>(null);
  const boards = data.data?.boards ?? [];
  const here = repositoryOf(location.pathname);

  const open = useCallback<Open>(
    (next = {}) => {
      setChosen(null);
      setOpened((n) => n + 1);
      setRequest({ ...next, repository: next.repository ?? here ?? undefined });
    },
    [here],
  );

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if (event.key.toLowerCase() !== "c") return;
      if (event.defaultPrevented || event.ctrlKey || event.metaKey || event.altKey) return;
      if (event.shiftKey || event.repeat || isTyping(event.target) || modalIsOpen()) return;
      event.preventDefault();
      open();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open]);

  useEffect(() => {
    if (said === null) return;
    const timer = setTimeout(() => setSaid(null), SAID_FOR);
    return () => clearTimeout(timer);
  }, [said]);

  const wanted = chosen ?? request?.repository ?? null;
  const board = boards.find((b) => b.repository.id === wanted) ?? boards[0];

  return (
    <NewIssueContext.Provider value={open}>
      {children}
      {board && (
        <NewIssueDialog
          key={`${opened}`}
          repository={board.repository.id}
          name={board.repository.name}
          repositories={boards.map((b) => b.repository)}
          onRepository={setChosen}
          board={board}
          parent={request?.parent ?? null}
          open={request !== null}
          onOpenChange={(next) => !next && setRequest(null)}
          onCreated={(card) => {
            if (request?.onCreated) request.onCreated(card);
            else setSaid(`Created "${card.title}"`);
          }}
        />
      )}
      {said && (
        <div className="pointer-events-none fixed inset-x-0 bottom-4 z-50 flex justify-center px-4">
          <Notice className="pointer-events-auto shadow-edge">{said}</Notice>
        </div>
      )}
    </NewIssueContext.Provider>
  );
}
