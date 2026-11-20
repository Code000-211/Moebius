/**
 * A card as a centred window over the page it was opened from, which stays behind a dim scrim: the board, the Inbox or
 * Fleet, with nothing squeezed (`CardWindowHost.tsx` mounts it for `?card=` on any route). It is one column: the id, "Open as page" (the card's own
 * page, `CardKeyPage.tsx`, which draws the same `CardContent`) and the way out; the title with the card's status icon, its owner, what its change costs (`+84 -12`) and its menu;
 * the properties of its issue as a compact block (`CardProperties.tsx`); the issue's text; the headline of its change,
 * which opens the whole diff in a sheet; what a person can do about it (approve, merge, send back, retry, open or stop
 * its run); its activity (comments, runs, moves and refusals as one timeline) and the script of its column,
 * read-only. A dialog the window opens (edit the issue, the diff, the menu's questions) lies over it, and Escape closes
 * the topmost layer only; Escape in a field leaves the field first. Closing hands the focus back to what opened it.
 * It is the web counterpart of the desktop board's dock plus the history screen; the script's editor comes with its page.
 */
import { ArrowsOutIcon, XIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { memo, type ReactNode, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Link } from "react-router";
import { cardRuns, script } from "../../api/calls/board";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Board, Card, InboxItem } from "../../api/types/board";
import { ChangeDelta, ChangesCard } from "../../diff/Changes";
import { Button } from "../../ui/components/Button";
import { IconButton } from "../../ui/components/IconButton";
import { Input } from "../../ui/components/Input";
import { KanbanIssueSection } from "../../ui/components/KanbanIssuePanel";
import { PrBadge } from "../../ui/components/PrBadge";
import { Avatar } from "../../ui/page/Avatar";
import { Field } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { StatusIcon, type StatusState } from "../../ui/page/StatusIcon";
import { CardMenu } from "./CardMenu";
import { CardProperties, CHECKS } from "./CardProperties";
import { CardText } from "./CardText";
import { cardName, cardPagePath } from "./cardKey";
import type { Decision } from "./decisions";
import { ActivitySection, EditIssueDialog } from "./IssueSections";
import {
  type CardStatus,
  cardStatus,
  changeOf,
  changesOfCard,
  changeWorktree,
  descendants,
  issueUrl,
  parentOf,
  pullRequestOf,
} from "./model";

/** The status icon of a card: what it asks of a person or is doing now first, then the column it stands in. */
export function iconOf(card: Card, status: CardStatus): StatusState {
  if (status === "live") return "running";
  if (status === "needs-you" || status === "stuck" || status === "held") return "needsYou";
  if (card.closed || card.column === "Done") return "done";
  if (card.column === "In review") return "inReview";
  if (card.column === "In progress") return "inProgress";
  return "notStarted";
}

/** What the card waits for from a person, as buttons; sending a change back asks why first. */
function Actions({
  card,
  item,
  busy,
  askFor,
  onAskAnswered,
  onDecide,
}: {
  card: Card;
  item: InboxItem;
  busy: boolean;
  /** The card a send-back is waiting on a reason for; this one, or none. */
  askFor: number | null;
  /** Says that the reason was sent or dropped, so the next send-back asks again. */
  onAskAnswered: () => void;
  onDecide: (decision: Decision) => void;
}) {
  const [rejecting, setRejecting] = useState(false);
  const [reason, setReason] = useState("");
  // The ask names the card it is about, so opening another card does not find this one's question waiting for it.
  useEffect(() => {
    if (askFor === card.id) setRejecting(true);
  }, [askFor, card.id]);
  const change = typeof item.data.change === "number" ? item.data.change : null;
  const number = card.number;

  if (rejecting && change !== null) {
    const ready = reason.trim() !== "";
    const send = () => {
      if (!ready || busy) return;
      onDecide({ kind: "reject", change, number, reason: reason.trim() });
      setRejecting(false);
      setReason("");
      onAskAnswered();
    };
    return (
      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault();
          send();
        }}
      >
        <Field label="Why? The next round is told">
          {(id) => (
            <Input id={id} value={reason} onChange={(e) => setReason(e.target.value)} autoFocus />
          )}
        </Field>
        <div className="flex flex-wrap gap-2">
          <Button type="submit" size="sm" disabled={!ready || busy}>
            Send back
          </Button>
          <Button
            type="button"
            size="sm"
            variant="ghost"
            onClick={() => {
              setRejecting(false);
              onAskAnswered();
            }}
          >
            Cancel
          </Button>
        </div>
      </form>
    );
  }

  return (
    <div className="flex flex-wrap gap-2">
      {item.kind === "approve_issue" && (
        <Button
          size="sm"
          disabled={busy}
          onClick={() => onDecide({ kind: "approve", card: card.id, number })}
        >
          Approve
        </Button>
      )}
      {item.kind === "ready_to_merge" && change !== null && (
        <>
          <Button
            size="sm"
            disabled={busy}
            onClick={() => onDecide({ kind: "merge", change, number })}
          >
            Merge
          </Button>
          <Button size="sm" variant="secondary" disabled={busy} onClick={() => setRejecting(true)}>
            Reject with a reason
          </Button>
        </>
      )}
      {(item.kind === "stuck" || item.kind === "change_closed") && (
        <Button
          size="sm"
          disabled={busy}
          onClick={() => onDecide({ kind: "retry", card: card.id, number })}
        >
          Retry
        </Button>
      )}
    </div>
  );
}

function QueryText({
  query,
  empty,
}: {
  query: { isPending: boolean; error: unknown };
  empty?: string;
}) {
  if (query.isPending) return <SkeletonLines lines={1} message="Reading" />;
  if (query.error) {
    return (
      <Notice tone="error">
        {isApiError(query.error) ? query.error.message : "Could not read this."}
      </Notice>
    );
  }
  return empty ? <p className="m-0 text-sm text-secondary">{empty}</p> : null;
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

  useEffect(() => {
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
        tabIndex={-1}
        className="fixed inset-0 z-sheet m-auto flex h-[calc(100%-7rem)] max-h-[56rem] w-full max-w-3xl flex-col overflow-hidden rounded-sheet bg-panel shadow-raised outline-none"
      >
        {children}
      </div>
    </>,
    document.body,
  );
}

/**
 * What a card shows, one column: its id and the way out, the title with its status icon, owner, change and menu, its
 * properties, the issue, its change, what a person can do about it, its run, its activity and the script of its
 * column. The window (`CardWindow`) and the card's own page (`CardKeyPage.tsx`) both draw it.
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
  /** The controls at the end of the bar over the card: what leads away from it, which only the window and the page know. */
  bar?: ReactNode;
  /** Whether the card scrolls inside its own box (the window's); the page scrolls with the page. */
  scrolls?: boolean;
}) {
  const [editing, setEditing] = useState(false);
  const repository = board.repository;
  const status = cardStatus(card, item, board);
  const runs = useQuery({ queryKey: keys.cardRuns(card.id), queryFn: () => cardRuns(card.id) });
  // The card runs the scripts of the project whose board it is on.
  const source = useQuery({
    queryKey: keys.script(card.project_id, card.column),
    queryFn: () => script(card.project_id, card.column),
  });
  // The worktree of the run that made the change.
  const worktree = changeWorktree(card, runs.data ?? []);
  const changes = changesOfCard(card);
  const owner = card.assignees[0] ?? card.claimed_by;
  const id = cardName(card, repository.forge !== "none");
  const url = issueUrl(repository, card);
  // A change of the card's other repositories is read from the board of that repository.
  const known = boards ?? [board];
  const pullRequestIn = (changeId: number, name: string | undefined) => {
    const where = known.find((b) => b.repository.name === name)?.repository ?? repository;
    const found = known.flatMap((b) => b.changes).find((c) => c.id === changeId);
    // The card's own change is also read from its own board when the boards are not all given.
    return pullRequestOf(
      where,
      found ?? (changeId === card.change?.id ? changeOf(board, card) : undefined),
    );
  };

  return (
    <>
      {/* Where the card is, and the two ways out of the window. */}
      <div className="flex h-12 shrink-0 items-center justify-between gap-2 border-b border-hairline px-6">
        {url ? (
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
        )}
        <div className="flex items-center gap-1">{bar}</div>
      </div>

      <div className={scrolls ? "min-h-0 flex-1 overflow-y-auto" : "pb-6"}>
        {/* What the card is, and who and what it is about. The title wraps, however long. */}
        <div className="flex items-start justify-between gap-4 px-6 pt-5 pb-3">
          <h2 className="m-0 min-w-0 text-2xl font-semibold tracking-tight text-text [overflow-wrap:anywhere]">
            {card.title}
          </h2>
          <div className="flex shrink-0 items-center gap-3">
            <StatusIcon state={iconOf(card, status)} />
            {owner && <Avatar handle={owner} className="size-6" />}
            {changes[0] && <ChangeDelta change={changes[0].id} className="text-sm" />}
            <CardMenu card={card} {...menu} />
          </div>
        </div>

        <CardProperties
          board={board}
          card={card}
          item={item}
          stopped={stopped}
          boards={boards}
          onOpenCard={onOpenCard}
        />

        {/* The issue is what the card asks for, and the card is only its title: without the body a person decides
              about a sentence they were never given. */}
        {/* A card with no text has no section to head: only the way to write some. */}
        {card.body.trim() !== "" ? (
          <KanbanIssueSection title="The issue">
            <CardText content={card.body} projectKey={card.project_key} className="max-w-prose" />
            <Button
              variant="secondary"
              size="sm"
              className="mt-4 w-fit"
              onClick={() => setEditing(true)}
            >
              Edit issue
            </Button>
          </KanbanIssueSection>
        ) : (
          <div className="px-6 pt-2 pb-2">
            <Button
              variant="secondary"
              size="sm"
              className="w-fit"
              onClick={() => setEditing(true)}
            >
              Edit issue
            </Button>
          </div>
        )}
        {/* Drawn only while it is open, so a closed one leaves nothing in the stack of layers. */}
        {editing && <EditIssueDialog key={card.id} card={card} open onOpenChange={setEditing} />}

        {/* A change is what a person came to the card to weigh, so it follows the issue it answers instead of
              waiting behind the runs that made it. */}
        {changes.length > 0 && (
          <KanbanIssueSection title="Changes">
            <div className="flex flex-col gap-4">
              {changes.map((change, index) => {
                const pullRequest = pullRequestIn(change.id, change.repository);
                return (
                  <div
                    key={change.id}
                    data-testid="card-change"
                    className="flex min-w-0 flex-col gap-2"
                  >
                    {changes.length > 1 && (
                      <p className="m-0 flex flex-wrap items-baseline gap-x-2 text-sm text-text">
                        <span className="font-medium">{change.repository}</span>
                        <span className="text-secondary">
                          {`Checks: ${CHECKS[change.checks] ?? change.checks}`}
                        </span>
                      </p>
                    )}
                    {pullRequest && (
                      <PrBadge
                        number={pullRequest.number}
                        url={pullRequest.url}
                        status={pullRequest.status}
                      />
                    )}
                    <ChangesCard
                      of={{ change: change.id }}
                      worktree={index === 0 ? worktree : null}
                    />
                  </div>
                );
              })}
            </div>
          </KanbanIssueSection>
        )}

        {item && (
          <KanbanIssueSection title="Your turn">
            <p className="m-0 mb-4 text-base text-text [overflow-wrap:anywhere]">{item.title}</p>
            <Actions
              card={card}
              item={item}
              busy={busy}
              askFor={askFor}
              onAskAnswered={onAskAnswered}
              onDecide={onDecide}
            />
          </KanbanIssueSection>
        )}

        {card.run && (
          <KanbanIssueSection title="Its run">
            <div className="flex flex-wrap gap-2">
              <Button asChild variant="secondary" size="sm" className="w-fit">
                <Link to={`/runs/${card.run.id}`}>Open live view</Link>
              </Button>
              {status === "live" && (
                <Button
                  variant="secondary"
                  size="sm"
                  className="w-fit"
                  disabled={busy}
                  onClick={() => menu.onStop(card)}
                >
                  Stop run
                </Button>
              )}
            </div>
          </KanbanIssueSection>
        )}

        <ActivitySection card={card} />

        <KanbanIssueSection title={`Script of ${card.column}`}>
          <QueryText query={source} />
          {source.data && (
            <>
              <p className="m-0 mb-3 text-sm text-secondary">
                {source.data.path} ·{" "}
                {source.data.source === "project" ? "this project's own" : "built in"}
              </p>
              <pre
                data-testid="script"
                className="m-0 max-h-48 overflow-auto rounded-control bg-code p-4 font-mono text-sm text-text"
              >
                {source.data.text}
              </pre>
              <Button asChild variant="secondary" size="sm" className="mt-4 w-fit">
                <Link
                  to={`/board/${repository.id}/scripts/${encodeURIComponent(card.column)}?project=${card.project_id}`}
                >
                  Open the script
                </Link>
              </Button>
            </>
          )}
        </KanbanIssueSection>
      </div>
    </>
  );
}

function CardWindowView(
  props: Omit<Parameters<typeof CardContent>[0], "bar" | "scrolls"> & { onClose: () => void },
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
            <IconButton
              icon={XIcon}
              iconWeight="regular"
              aria-label="Close the inspector"
              onClick={onClose}
            />
          </>
        }
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
