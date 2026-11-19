/**
 * The inspector dock of the board: the card chosen, with the issue it came from and the change it offered — the
 * headline of its diff, which opens the whole diff in a sheet — then what a person can do about it (approve, merge,
 * send back, retry, open or stop its run), its comments, its runs, its history and the script of its column,
 * read-only. Its facts and the properties of its issue are in the page's right column (`CardProperties.tsx`), which
 * the dock puts there while it is open.
 * It is a drawer beside the board, so the board stays readable behind it: no border, the
 * groups apart by space, and every control labelled. It is the web counterpart of the desktop board's dock plus the
 * history screen; the script's editor comes with its page.
 */
import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { Link } from "react-router";
import { cardRuns, history, script } from "../../api/calls/board";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Board, Card, InboxItem } from "../../api/types/board";
import { ChangesCard } from "../../diff/Changes";
import { Markdown } from "../../ui/chat/Markdown";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { KanbanIssuePanel, KanbanIssueSection } from "../../ui/components/KanbanIssuePanel";
import { PrBadge } from "../../ui/components/PrBadge";
import { ago } from "../../ui/lib/format";
import { useNow } from "../../ui/lib/useNow";
import { Field } from "../../ui/page/Field";
import { MarkLabel } from "../../ui/page/MarkLabel";
import { Notice } from "../../ui/page/Notice";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { CardMenu } from "./CardMenu";
import { CardProperties, STATUS_MARK } from "./CardProperties";
import type { Decision } from "./decisions";
import { CommentsSection, EditIssueDialog } from "./IssueSections";
import {
  cardStatus,
  changeOf,
  changeWorktree,
  describeHistory,
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
  const entries = useQuery({ queryKey: keys.history(card.id), queryFn: () => history(card.id) });
  const source = useQuery({
    queryKey: keys.script(repository.id, card.column),
    queryFn: () => script(repository.id, card.column),
  });
  // The history of a card is read once and stays open, so the age of the last thing it did only moves with a clock of
  // its own.
  const now = useNow();
  // The worktree of the run that made the change, from the runs read for the list below anyway.
  const worktree = changeWorktree(card, runs.data ?? []);
  const pullRequest = pullRequestOf(repository, changeOf(board, card));

  return (
    <KanbanIssuePanel
      displayId={`#${card.number}`}
      issueUrl={issueUrl(repository, card)}
      title={card.title}
      status={<MarkLabel mark={STATUS_MARK[status]} word={STATUS_LABELS[status]} />}
      menu={<CardMenu card={card} {...menu} />}
      onClose={onClose}
    >
      <CardProperties board={board} card={card} item={item} onOpenCard={onOpenCard} />

      {/* The issue is what the card asks for, and the card is only its title: without the body a person decides
          about a sentence they were never given. */}
      <KanbanIssueSection title="The issue">
        {card.body.trim() !== "" && (
          <Markdown content={card.body} keepNewlines className="max-w-prose" />
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
      {card.change && (
        <KanbanIssueSection title="Changes">
          {pullRequest && (
            <PrBadge
              number={pullRequest.number}
              url={pullRequest.url}
              status={pullRequest.status}
              className="mb-3"
            />
          )}
          <ChangesCard of={{ change: card.change.id }} worktree={worktree} />
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

      <CommentsSection card={card} />

      <KanbanIssueSection title="Runs">
        <QueryText query={runs} empty={runs.data?.length === 0 ? "No runs yet" : undefined} />
        {/* biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with
            no marker from the accessibility tree entirely */}
        <ul role="list" className="m-0 flex list-none flex-col gap-2 p-0">
          {(runs.data ?? []).map((run) => (
            <li key={run.id} className="flex min-w-0 flex-wrap items-center gap-3">
              <Link
                to={`/runs/${run.id}`}
                className="-ml-2 inline-flex min-h-8 items-center rounded-control px-2 text-base font-semibold text-text underline decoration-hairline-strong underline-offset-4 hover:decoration-current"
              >
                {`Run ${run.id}`}
              </Link>
              <span className="min-w-0 break-words text-sm text-secondary tabular-nums">
                {run.agent} · {run.purpose} · {run.status}
              </span>
            </li>
          ))}
        </ul>
      </KanbanIssueSection>

      <KanbanIssueSection title="History">
        <QueryText
          query={entries}
          empty={entries.data?.length === 0 ? "No history yet" : undefined}
        />
        {/* biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with
            no marker from the accessibility tree entirely */}
        <ol role="list" className="m-0 flex list-none flex-col gap-2 p-0">
          {(entries.data ?? []).map((entry, index) => (
            // The entries have no id; they are shown in the order the server sends them.
            // biome-ignore lint/suspicious/noArrayIndexKey: the history is a fixed, ordered list
            <li key={index} className="flex min-w-0 gap-4">
              <span className="w-16 shrink-0 text-sm text-secondary tabular-nums">
                {ago(now, entry.at)}
              </span>
              <span className="min-w-0 flex-1 text-base text-text [overflow-wrap:anywhere]">
                {describeHistory(entry)}
              </span>
            </li>
          ))}
        </ol>
      </KanbanIssueSection>

      <KanbanIssueSection title={`Script of ${card.column}`}>
        <QueryText query={source} />
        {source.data && (
          <>
            <p className="m-0 mb-3 text-sm text-secondary">
              {source.data.path} ·{" "}
              {source.data.source === "repository" ? "this repository's own" : "built in"}
            </p>
            <pre
              data-testid="script"
              className="m-0 max-h-48 overflow-auto rounded-control bg-code p-4 font-mono text-sm text-text"
            >
              {source.data.text}
            </pre>
            <Button asChild variant="secondary" size="sm" className="mt-4 w-fit">
              <Link to={`/board/${repository.id}/scripts/${encodeURIComponent(card.column)}`}>
                Open the script
              </Link>
            </Button>
          </>
        )}
      </KanbanIssueSection>
    </KanbanIssuePanel>
  );
}
