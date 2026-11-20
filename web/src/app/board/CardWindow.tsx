/**
 * A card as a centred window over the page it was opened from, which stays behind a dim scrim: the board, the Inbox or
 * Fleet, with nothing squeezed (`CardWindowHost.tsx` mounts it for `?card=` on any route). It is one column: the id, "Open as page"
 * and the way out; the title with the card's status icon, its owner, what its change costs (`+84 -12`) and its menu;
 * the properties of its issue as a compact block (`CardProperties.tsx`); the issue's text; the headline of its change,
 * which opens the whole diff in a sheet; what a person can do about it (approve, merge, send back, retry, open or stop
 * its run); its activity (comments, runs, moves and refusals as one timeline) and the script of its column,
 * read-only. A dialog the window opens (edit the issue, the diff, the menu's questions) lies over it, and Escape closes
 * the topmost layer only; Escape in a field leaves the field first. Closing hands the focus back to what opened it.
 * It is the web counterpart of the desktop board's dock plus the history screen; the script's editor comes with its page.
 */
import { ArrowsOutIcon, XIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { memo, useEffect, useState } from "react";
import { Link } from "react-router";
import { cardRuns, script } from "../../api/calls/board";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Board, Card, InboxItem } from "../../api/types/board";
import { ChangeDelta, ChangesCard } from "../../diff/Changes";
import { Button } from "../../ui/components/Button";
import { Dialog, DialogContent } from "../../ui/components/Dialog";
import { IconButton } from "../../ui/components/IconButton";
import { Input } from "../../ui/components/Input";
import { KanbanIssueSection } from "../../ui/components/KanbanIssuePanel";
import { PrBadge } from "../../ui/components/PrBadge";
import { Avatar } from "../../ui/page/Avatar";
import { Field } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { StatusIcon, type StatusState } from "../../ui/page/StatusIcon";
import { boardPath } from "../pages";
import { CardMenu } from "./CardMenu";
import { CardProperties, CHECKS } from "./CardProperties";
import { CardText } from "./CardText";
import { cardName } from "./cardKey";
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

function CardWindowView({
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
  onClose,
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
  onClose: () => void;
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
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent
        data-card-window=""
        hideCloseButton
        // The window is named for what it is, not by the title it holds, and has no description to read out.
        aria-label="Card inspector"
        aria-labelledby={undefined}
        aria-describedby={undefined}
        onEscapeKeyDown={(event) => {
          // A field keeps its Escape for leaving itself; the card stays until the second one.
          const target = event.target as HTMLElement;
          if (
            target.tagName === "INPUT" ||
            target.tagName === "TEXTAREA" ||
            target.isContentEditable
          ) {
            event.preventDefault();
            target.blur();
          }
        }}
        className="h-[calc(100%-4rem)] max-h-[56rem] max-w-3xl gap-0 overflow-hidden p-0"
      >
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
          <div className="flex items-center gap-1">
            {/* Until the card has a page of its own, "as a page" is the board with the card open. */}
            <Button asChild variant="ghost" size="sm">
              <Link to={`${boardPath(repository.id)}?card=${card.id}`}>
                <ArrowsOutIcon className="size-4" aria-hidden="true" />
                Open as page
              </Link>
            </Button>
            <IconButton icon={XIcon} aria-label="Close the inspector" onClick={onClose} />
          </div>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto">
          {/* What the card is, and who and what it is about. The title wraps, however long. */}
          <div className="flex items-start justify-between gap-4 px-6 pt-5 pb-3">
            <h2 className="m-0 min-w-0 text-2xl font-semibold tracking-tight text-text [overflow-wrap:anywhere]">
              {card.title}
            </h2>
            <div className="flex shrink-0 items-center gap-3 pt-1.5">
              <StatusIcon state={iconOf(card, status)} />
              {owner && <Avatar handle={owner} className="size-6" />}
              {changes[0] && <ChangeDelta change={changes[0].id} className="text-sm" />}
              <CardMenu card={card} {...menu} className="-mr-2" />
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
          <KanbanIssueSection title="The issue">
            {card.body.trim() !== "" && (
              <CardText content={card.body} projectKey={card.project_key} className="max-w-prose" />
            )}
            <Button
              variant="secondary"
              size="sm"
              className="mt-4 w-fit"
              onClick={() => setEditing(true)}
            >
              Edit issue
            </Button>
            <EditIssueDialog key={card.id} card={card} open={editing} onOpenChange={setEditing} />
          </KanbanIssueSection>

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
      </DialogContent>
    </Dialog>
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
