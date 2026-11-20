/**
 * The inspector dock of the board: the card chosen, with the issue it came from and the change it offered — the
 * headline of its diff, which opens the whole diff in a sheet — then what a person can do about it (approve, merge,
 * send back, retry, open or stop its run), its activity (comments, runs, moves and refusals as one timeline) and the script of its column,
 * read-only. Its facts and the properties of its issue are in the page's right column (`CardProperties.tsx`), which
 * the dock puts there while it is open.
 * It is a drawer beside the board, so the board stays readable behind it: no border, the
 * groups apart by space, and every control labelled. It is the web counterpart of the desktop board's dock plus the
 * history screen; the script's editor comes with its page.
 */
import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { Link } from "react-router";
import { cardRuns, script } from "../../api/calls/board";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Board, Card, InboxItem } from "../../api/types/board";
import { ChangesCard } from "../../diff/Changes";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { KanbanIssuePanel, KanbanIssueSection } from "../../ui/components/KanbanIssuePanel";
import { PrBadge } from "../../ui/components/PrBadge";
import { Field } from "../../ui/page/Field";
import { MarkLabel } from "../../ui/page/MarkLabel";
import { Notice } from "../../ui/page/Notice";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { CardMenu } from "./CardMenu";
import { CardProperties, CHECKS, STATUS_MARK } from "./CardProperties";
import { CardText } from "./CardText";
import { cardName } from "./cardKey";
import type { Decision } from "./decisions";
import { ActivitySection, EditIssueDialog } from "./IssueSections";
import {
  cardStatus,
  changeOf,
  changesOfCard,
  changeWorktree,
  issueUrl,
  pullRequestOf,
  STATUS_LABELS,
} from "./model";

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

export function Dock({
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
  /** Shows another issue, such as the parent or a sub-issue, in the dock. */
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
  // A change of the card's other repositories is read from the board of that repository.
  const known = boards ?? [board];
  const pullRequestIn = (id: number, name: string | undefined) => {
    const where = known.find((b) => b.repository.name === name)?.repository ?? repository;
    const found = known.flatMap((b) => b.changes).find((c) => c.id === id);
    // The card's own change is also read from its own board when the boards are not all given.
    return pullRequestOf(
      where,
      found ?? (id === card.change?.id ? changeOf(board, card) : undefined),
    );
  };

  return (
    <KanbanIssuePanel
      displayId={cardName(card, repository.forge !== "none")}
      issueUrl={issueUrl(repository, card)}
      title={card.title}
      status={<MarkLabel mark={STATUS_MARK[status]} word={STATUS_LABELS[status]} />}
      menu={<CardMenu card={card} {...menu} />}
      onClose={onClose}
    >
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
    </KanbanIssuePanel>
  );
}
