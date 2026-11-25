/**
 * A link to a card in a text, such as `#18` or `WEB-24` in a comment, a chat or the log of a run. It is drawn as a link to the
 * card on its board and shows a preview card of the card: its project, column, people, labels and the run in progress,
 * if there is one. The preview opens after a short wait with the pointer over the link and at once when the link takes
 * the keyboard focus, and closes with Escape, when the pointer or the focus leaves, or on a click, which opens the card.
 *
 * A number that names no card of the snapshot stays the plain text it was; so does every number where there is no
 * snapshot to look in (a text drawn outside the app's shell). Several repositories have an issue with the same
 * number: `CardScope` names the repository a text belongs to, and `#18` is its issue of that number, else the card of
 * that number in the project of that repository; a text with no scope takes the first board that has the number. A key such
 * as `WEB-24` names its card on any board.
 */
import { QueryClientContext, useQuery } from "@tanstack/react-query";
import {
  type ComponentProps,
  createContext,
  Fragment,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useId,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import { Link, useInRouterContext, useLocation } from "react-router";
import { snapshot } from "../../api/calls/board";
import { keys } from "../../api/keys";
import type { Board, Card } from "../../api/types/board";
import { ColumnMark } from "../../app/board/ColumnMark";
import { withCard } from "../../app/board/cardAddress";
import { cardName } from "../../app/board/cardKey";
import { repositoryBoardPath } from "../../app/pages";
import { RunningDots } from "../components/RunningDots";

/** How long the pointer rests on a link before its preview opens. */
export const PREVIEW_DELAY_MS = 400;

/** The repository the text being drawn belongs to, as its id; `null` when it belongs to none. */
export const CardScope = createContext<number | null>(null);

/** A `#18` or a card key such as `WEB-24`; a key is a few capitals and digits, a hyphen and digits. */
const REFERENCE = /(?<![\w&#/-])(?:#(\d+)|([A-Z][A-Z0-9]{1,9})-(\d+))\b(?!-\d)/g;

/** The card a key and a number in its project name, on any board. */
export function findKeyed(
  boards: readonly Board[],
  key: string,
  number: number,
): { board: Board; card: Card } | undefined {
  for (const board of boards) {
    const card = board.cards.find((c) => c.project_key === key && c.project_number === number);
    if (card) return { board, card };
  }
  return undefined;
}

/**
 * The card `#18` names: the issue with that number in the scope's repository, else the card with that number in the
 * scope's project, else the first board that has either.
 */
export function findCard(
  boards: readonly Board[],
  number: number,
  scope: number | null,
): { board: Board; card: Card } | undefined {
  const own = boards.find((b) => b.repository.id === scope);
  const issue = own?.cards.find((c) => c.number === number);
  if (own && issue) return { board: own, card: issue };
  const home = own?.repository.home_project_id;
  for (const board of boards) {
    // Without a scope, no project is the text's: the first card with the number in its project is meant.
    const card = board.cards.find(
      (c) => c.project_number === number && (scope === null || c.project_id === home),
    );
    if (card) return { board, card };
  }
  const ordered =
    scope === null
      ? boards
      : [
          ...boards.filter((b) => b.repository.id === scope),
          ...boards.filter((b) => b.repository.id !== scope),
        ];
  for (const board of ordered) {
    const card = board.cards.find((c) => c.number === number);
    if (card) return { board, card };
  }
  return undefined;
}

/** `children` with every `#18` in its text turned into a link to a card with a preview. */
export function linkCards(children: ReactNode): ReactNode {
  if (Array.isArray(children)) {
    return children.map((child, index) => (
      // biome-ignore lint/suspicious/noArrayIndexKey: the pieces of a paragraph have no identity but their place
      <Fragment key={index}>{linkCards(child)}</Fragment>
    ));
  }
  if (typeof children !== "string") return children;
  const pieces: ReactNode[] = [];
  let from = 0;
  for (const found of children.matchAll(REFERENCE)) {
    const at = found.index ?? 0;
    if (at > from) pieces.push(children.slice(from, at));
    pieces.push(
      found[1] ? (
        <CardRef key={at} number={Number(found[1])} />
      ) : (
        <CardRef key={at} number={Number(found[3])} projectKey={found[2]} />
      ),
    );
    from = at + found[0].length;
  }
  if (from === 0) return children;
  if (from < children.length) pieces.push(children.slice(from));
  return pieces;
}

/** One `#18`: a link with a preview when the snapshot has the card, the plain text when not. */
export function CardRef({ number, projectKey }: { number: number; projectKey?: string }) {
  const client = useContext(QueryClientContext);
  const text = projectKey ? `${projectKey}-${number}` : `#${number}`;
  if (!client) return <>{text}</>;
  return <CardRefLink number={number} projectKey={projectKey} text={text} />;
}

function CardRefLink({
  number,
  projectKey,
  text,
}: {
  number: number;
  projectKey?: string;
  text: string;
}) {
  const scope = useContext(CardScope);
  const { data } = useQuery({ queryKey: keys.snapshot, queryFn: snapshot });
  const found = !data
    ? undefined
    : projectKey
      ? findKeyed(data.boards, projectKey, number)
      : findCard(data.boards, number, scope);
  if (!found) return <>{text}</>;
  return <PreviewLink text={text} board={found.board} card={found.card} />;
}

/** A link that opens the card as the window over the page it is on, which stays as it is. */
function OpenHere({
  card,
  children,
  ...props
}: { card: number; children: ReactNode } & Omit<ComponentProps<typeof Link>, "to">) {
  const { pathname, search } = useLocation();
  return (
    <Link
      to={{ pathname, search: withCard(new URLSearchParams(search), card).toString() }}
      {...props}
    >
      {children}
    </Link>
  );
}

function PreviewLink({ text, board, card }: { text: string; board: Board; card: Card }) {
  const [open, setOpen] = useState(false);
  const [at, setAt] = useState<{ left: number; top: number } | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const anchor = useRef<HTMLAnchorElement>(null);
  const id = useId();
  const inRouter = useInRouterContext();
  const href = repositoryBoardPath(board.repository, card.id);

  const show = useCallback(() => {
    clearTimeout(timer.current);
    const box = anchor.current?.getBoundingClientRect();
    if (box) setAt({ left: box.left, top: box.bottom + 4 });
    setOpen(true);
  }, []);
  const hide = useCallback(() => {
    clearTimeout(timer.current);
    setOpen(false);
  }, []);
  const wait = useCallback(() => {
    clearTimeout(timer.current);
    timer.current = setTimeout(show, PREVIEW_DELAY_MS);
  }, [show]);

  useEffect(() => () => clearTimeout(timer.current), []);
  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") hide();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, hide]);

  const props = {
    ref: anchor,
    className:
      "text-accent-text underline-offset-4 hover:underline focus-visible:underline rounded-small",
    "aria-describedby": open ? id : undefined,
    onMouseEnter: wait,
    onMouseLeave: hide,
    onFocus: show,
    onBlur: hide,
    onClick: hide,
  };
  return (
    <>
      {inRouter ? (
        <OpenHere card={card.id} {...props}>
          {text}
        </OpenHere>
      ) : (
        <a href={href} {...props}>
          {text}
        </a>
      )}
      {open && at
        ? createPortal(<CardPreview id={id} board={board} card={card} at={at} />, document.body)
        : null}
    </>
  );
}

/** The people of a card: who made it, who claimed it, who holds it. */
function peopleOf(card: Card): string[] {
  const names = [card.author, card.claimed_by, card.held ? card.held.by : null];
  return [...new Set(names.filter((n): n is string => !!n))].map((n) =>
    n.startsWith("agent:") ? n.slice("agent:".length) : n,
  );
}

/** The preview card itself: what the card is, in a few lines. */
export function CardPreview({
  id,
  board,
  card,
  at,
}: {
  id: string;
  board: Board;
  card: Card;
  at: { left: number; top: number };
}) {
  const run =
    card.run && (card.run.status === "running" || card.run.status === "starting") ? card.run : null;
  const people = peopleOf(card);
  return (
    <div
      id={id}
      role="tooltip"
      data-testid="card-preview"
      style={{ left: Math.max(8, at.left), top: at.top }}
      className="glass fixed z-tip flex w-72 flex-col gap-2 rounded-card p-3 text-xs text-text shadow-raised"
    >
      <span className="text-secondary">
        {board.repository.project ?? board.repository.name} ·{" "}
        {cardName(card, board.repository.forge !== "none")}
      </span>
      <span className="text-sm font-medium text-text">{card.title}</span>
      <span className="flex items-center gap-2 text-secondary">
        <ColumnMark name={card.column} counts={card.column_class} />
        {card.column}
      </span>
      {card.labels.length > 0 ? (
        <span className="text-secondary">{card.labels.join(", ")}</span>
      ) : null}
      {people.length > 0 ? <span className="text-secondary">{people.join(", ")}</span> : null}
      {run ? (
        <span className="flex items-center gap-2 text-progress-text" data-testid="card-preview-run">
          <RunningDots />
          {run.agent} is running: {run.purpose}
        </span>
      ) : null}
    </div>
  );
}
