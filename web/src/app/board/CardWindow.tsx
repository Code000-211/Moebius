/**
 * A card as a centred window over the page it was opened from, which stays behind a dim scrim: the board, the Inbox or
 * Fleet, with nothing squeezed (`CardWindowHost.tsx` mounts it for `?card=` on any route). It is one column: the id, "Open as page" (the card's own
 * page, `CardKeyPage.tsx`, which draws the same `CardContent`) and the way out; the title with the card's status icon, its owner, what its change costs (`+84 -12`) and its menu, which holds
 * the edit of the issue and every property (`CardDetails.tsx`); the thread as a timeline, where a run and a review are
 * one collapsed line each and what waits for a person is one tinted block with its buttons (`NeedsYou.tsx`); and the
 * reply field, pinned at the foot while the body scrolls. A dialog the window opens (edit the issue, the properties, the
 * menu's questions) lies over it, and Escape closes the topmost layer only; Escape in a field leaves the field first.
 * Closing hands the focus back to what opened it.
 */
import { ArrowsOutIcon } from "@phosphor-icons/react";
import { memo, type ReactNode, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Link } from "react-router";
import { cardClass, isDone } from "../../api/columns";
import type { Board, Card, InboxItem } from "../../api/types/board";
import { ChangeDelta } from "../../diff/Changes";
import { Button } from "../../ui/components/Button";
import { WindowHeader } from "../../ui/components/WindowHeader";
import { Avatar } from "../../ui/page/Avatar";
import { StatusIcon, type StatusState } from "../../ui/page/StatusIcon";
import { CardDetails } from "./CardDetails";
import { CardMenu } from "./CardMenu";
import { cardName, cardPagePath } from "./cardKey";
import type { Decision } from "./decisions";
import { FollowedRequest } from "./FollowedRequest";
import { FrozenNotice } from "./FrozenNotice";
import { CardThread, EditIssueDialog, ReplyField } from "./IssueSections";
import {
  type CardStatus,
  cardStatus,
  changesOfCard,
  descendants,
  elsewhereBy,
  issueUrl,
  parentOf,
} from "./model";
import { DONE_ASKED } from "./model/inbox";
import { NeedsYouBlock } from "./NeedsYou";
import { PullFields } from "./PullFields";

/** The status icon of a card: what it asks of a person or is doing now first, then the column it stands in. */
export function iconOf(card: Card, status: CardStatus): StatusState {
  if (status === "live") return "running";
  if (status === "needs-you" || status === "stuck" || status === "held") return "needsYou";
  if (card.closed || isDone(card)) return "done";
  if (cardClass(card) === "In review") return "inReview";
  if (cardClass(card) === "In progress") return "inProgress";
  return "notStarted";
}

/** The tabbable elements inside `box`, in order. */
function tabbables(box: HTMLElement): HTMLElement[] {
  return [
    ...box.querySelectorAll<HTMLElement>(
      'a[href], button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), summary, [tabindex]:not([tabindex="-1"])',
    ),
  ].filter((el) => el.offsetParent !== null);
}

/**
 * The window's frame: a scrim that dims the page (no blur) and a solid, centred dialog over it that is modal for the
 * keyboard and the focus. It is not a Radix layer on purpose: Radix gives the Escape key to one layer by the order
 * they mounted in, and a window that the page opens other windows over (the palette, a new issue, a question about
 * blocking) must not take that key from them. So Escape is heard here after every layer over the window has had it, and
 * closes the window only when nothing marked the key as handled and the window is the topmost dialog. The focus
 * goes into the window, stays in it on Tab, and returns to what opened it when it closes.
 */
function WindowFrame({ onClose, children }: { onClose: () => void; children: ReactNode }) {
  const box = useRef<HTMLDivElement>(null);
  // Noted while the window renders for the first time, before anything in it takes the focus.
  const opener = useRef<Element | null>(
    typeof document === "undefined" ? null : document.activeElement,
  );
  const close = useRef(onClose);
  close.current = onClose;

  // A layout effect, so the keys are heard from the frame the window is first on screen: a passive effect runs a
  // moment after it, and an Escape in that moment would leave the window open.
  useLayoutEffect(() => {
    const element = box.current;
    if (!element) return;
    // The window takes the focus, so the keys of the card in it are the page's.
    if (!element.contains(document.activeElement)) element.focus();
    function onKey(event: KeyboardEvent) {
      if (event.key === "Escape") {
        if (event.defaultPrevented) return;
        const open = document.querySelectorAll('[role="dialog"][data-state="open"]');
        if (open[open.length - 1] !== element) return;
        const target = event.target as HTMLElement;
        // A field of the window keeps its Escape for leaving itself; the card stays until the second one.
        if (
          element?.contains(target) &&
          (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable)
        )
          return;
        event.preventDefault();
        close.current();
      } else if (event.key === "Tab" && element) {
        const open = document.querySelectorAll('[role="dialog"][data-state="open"]');
        if (open[open.length - 1] !== element) return;
        const inside = tabbables(element);
        if (inside.length === 0) return;
        const first = inside[0];
        const last = inside[inside.length - 1];
        const at = document.activeElement;
        if (event.shiftKey && (at === first || at === element)) {
          event.preventDefault();
          last?.focus();
        } else if (!event.shiftKey && at === last) {
          event.preventDefault();
          first?.focus();
        } else if (!element.contains(at)) {
          event.preventDefault();
          first?.focus();
        }
      }
    }
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("keydown", onKey);
      const back = opener.current;
      if (back instanceof HTMLElement && back.isConnected && back !== document.body) back.focus();
    };
  }, []);

  return createPortal(
    <>
      <div
        aria-hidden="true"
        data-state="open"
        className="fixed inset-0 z-scrim bg-scrim"
        onPointerDown={onClose}
      />
      <div
        ref={box}
        role="dialog"
        aria-modal="true"
        aria-label="Card inspector"
        data-state="open"
        data-card-window=""
        data-audit="flush"
        tabIndex={-1}
        className="fixed inset-0 z-sheet m-auto flex h-[calc(100%-7rem)] max-h-[56rem] w-full max-w-3xl flex-col overflow-hidden rounded-window bg-panel shadow-window outline-none"
      >
        {children}
      </div>
    </>,
    document.body,
  );
}

/**
 * What a card shows, one column: its id and the way out, the title with its status icon, owner, change and menu (which
 * holds the edit of the issue and every property), the thread of the issue as a timeline, and the reply field at the
 * foot. The window (`CardWindow`) and the card's own page (`CardKeyPage.tsx`) both draw it.
 */
export function CardContent({
  board,
  card,
  item,
  stopped,
  boards,
  busy,
  askFor,
  onAskAnswered,
  onDecide,
  menu,
  onOpenCard,
  bar,
  onClose,
  back,
  scrolls = true,
}: {
  board: Board;
  card: Card;
  item: InboxItem | undefined;
  /** The card's open `merge_stopped` item, which says what is merged and what is still open. */
  stopped?: InboxItem;
  /** Every board: a change in another repository of the card is on that repository's board. Only this one when not given. */
  boards?: Board[];
  busy: boolean;
  /** The card a drop asked to send back; the reason form opens for that card alone. */
  askFor: number | null;
  onAskAnswered: () => void;
  onDecide: (decision: Decision) => void;
  /** What the card's menu does. */
  menu: {
    onBlock: (card: Card) => void;
    onUnblock: (card: Card) => void;
    onStop: (card: Card) => void;
    onDelete: (card: Card) => void;
    onAddSubIssue: (card: Card) => void;
    onMoveParent: (card: Card) => void;
  };
  /** Shows another issue, such as the parent or a sub-issue, in the window. */
  onOpenCard: (card: Card) => void;
  /** The controls before the close icon in the bar over the card: what leads away from it, which only the window and the page know. */
  bar?: ReactNode;
  /** The way out, last in the bar; the card's own page has none. */
  onClose?: () => void;
  /** The way back, at the left of the title, for the card shown as a page inside another. */
  back?: ReactNode;
  /** Whether the card scrolls inside its own box (the window's); the page scrolls with the page. */
  scrolls?: boolean;
}) {
  const [editing, setEditing] = useState(false);
  const [details, setDetails] = useState(false);
  const reply = useRef<HTMLFormElement>(null);
  const repository = board.repository;
  const status = cardStatus(card, item, board);
  const changes = changesOfCard(card);
  const owner = card.assignees[0] ?? elsewhereBy(card) ?? card.claimed_by;
  const id = cardName(card, repository.forge !== "none");
  const url = issueUrl(repository, card);

  return (
    <>
      {/* Where the card is, and the two ways out of the window. */}
      <WindowHeader
        leading={
          url ? (
            <a
              href={url}
              target="_blank"
              rel="noopener noreferrer"
              className="-ml-2 inline-flex min-h-7 items-center rounded-control px-2 font-mono text-sm text-secondary transition-colors duration-(--motion-routine) ease-out hover:bg-hover hover:text-text"
            >
              {id}
            </a>
          ) : (
            <span className="font-mono text-sm text-secondary">{id}</span>
          )
        }
        trailing={bar}
        onClose={onClose}
        closeLabel="Close the inspector"
      />

      <div className={scrolls ? "min-h-0 flex-1 overflow-y-auto" : ""}>
        {/* What the card is, and who and what it is about. The title wraps, however long. */}
        <div className="flex items-start justify-between gap-4 px-6 pt-5 pb-4">
          <div className="flex min-w-0 items-start gap-2">
            {back}
            <h2 className="m-0 min-w-0 text-2xl font-semibold tracking-tight text-text [overflow-wrap:anywhere]">
              {card.title}
            </h2>
          </div>
          <div className="flex shrink-0 items-center gap-3">
            <StatusIcon state={iconOf(card, status)} />
            {owner && <Avatar handle={owner} className="size-6" />}
            {changes[0] && <ChangeDelta change={changes[0].id} className="text-md" />}
            <CardMenu
              card={card}
              {...menu}
              onEdit={() => setEditing(true)}
              onProperties={() => setDetails(true)}
            />
          </div>
        </div>

        {board.repository.frozen && (
          <div className="px-6 pb-4">
            <FrozenNotice frozen={board.repository.frozen} />
          </div>
        )}

        <FollowedRequest
          board={board}
          card={card}
          busy={busy}
          // While the assignee's /done waits, "Release it" of the block below is the same action.
          onRelease={
            item?.kind === DONE_ASKED
              ? undefined
              : () => onDecide({ kind: "release", card: card.id, number: card.number })
          }
          onEndRound={() => {
            const change = card.change?.id;
            if (change !== undefined) onDecide({ kind: "end_round", change, number: card.number });
          }}
        />

        <PullFields change={card.change} />

        <div className="px-6 pb-6">
          <CardThread
            card={card}
            item={item}
            live={status === "live"}
            busy={busy}
            onStop={() => menu.onStop(card)}
            needsYou={(waiting) => (
              <NeedsYouBlock
                card={card}
                item={waiting}
                busy={busy}
                askFor={askFor}
                onAskAnswered={onAskAnswered}
                onDecide={onDecide}
                onReply={() => reply.current?.querySelector("textarea")?.focus()}
              />
            )}
          />
        </div>
      </div>

      {/* The reply field stays at the foot: in the window it is outside the scrolling body, on the page it sticks. */}
      <div
        className={`shrink-0 border-t border-hairline bg-panel px-6 py-4 ${scrolls ? "" : "sticky bottom-0"}`}
      >
        <ReplyField card={card} fieldRef={reply} />
      </div>

      {/* Drawn only while open, so a closed one leaves nothing in the stack of layers. */}
      {editing && <EditIssueDialog key={card.id} card={card} open onOpenChange={setEditing} />}
      {details && (
        <CardDetails
          board={board}
          card={card}
          item={item}
          stopped={stopped}
          boards={boards}
          onOpenCard={onOpenCard}
          open
          onOpenChange={setDetails}
        />
      )}
    </>
  );
}

function CardWindowView(
  props: Omit<Parameters<typeof CardContent>[0], "bar" | "onClose" | "scrolls"> & {
    onClose: () => void;
  },
) {
  const { onClose, ...rest } = props;
  return (
    <WindowFrame onClose={onClose}>
      <CardContent
        {...rest}
        bar={
          <>
            <Button asChild variant="ghost" size="sm">
              <Link to={cardPagePath(rest.card)}>
                <ArrowsOutIcon className="size-4" aria-hidden="true" />
                Open as page
              </Link>
            </Button>
          </>
        }
        onClose={onClose}
      />
    </WindowFrame>
  );
}

type CardWindowProps = Parameters<typeof CardWindowView>[0];

/** Whether two lists hold the same cards, by identity. */
const sameCards = (a: readonly Card[], b: readonly Card[]) =>
  a.length === b.length && a.every((card, at) => card === b[at]);

/** Whether two reads of a board say the same about `card`: what the window reads of the board, and nothing else. */
function sameForCard(was: Board, now: Board, card: Card): boolean {
  const key = String(card.id);
  return (
    was.repository === now.repository &&
    was.round_limit === now.round_limit &&
    was.waiting[key] === now.waiting[key] &&
    was.changes === now.changes &&
    parentOf(was, card) === parentOf(now, card) &&
    sameCards(descendants(was, card), descendants(now, card))
  );
}

/**
 * The window draws again when its card or anything it was handed changes, and when the board changes in what it reads of it:
 * the repository, the card's parent and sub-issues, the changes and what the card waits for. A read of the snapshot in
 * which another card changed leaves it alone; the window is the heaviest part of the page.
 */
function sameWindow(was: CardWindowProps, now: CardWindowProps): boolean {
  for (const key of Object.keys(now) as (keyof CardWindowProps)[]) {
    if (key === "board" || key === "boards") continue;
    if (!Object.is(was[key], now[key])) return false;
  }
  if (!sameForCard(was.board, now.board, now.card)) return false;
  const [before, after] = [was.boards ?? [], now.boards ?? []];
  return (
    before.length === after.length &&
    before.every(
      (each, at) =>
        each.repository === after[at]?.repository && each.changes === after[at]?.changes,
    )
  );
}

export const CardWindow = memo(CardWindowView, sameWindow);
