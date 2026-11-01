/**
 * The inspector dock of the board: the card chosen, with what a person can do about it (approve, merge, send back,
 * retry), where it stands (run, guard, state), its runs, its history and the script of its column, read-only. It is
 * the web counterpart of the desktop board's dock plus the history screen; the script's editor comes with its page.
 */
import { useQuery } from "@tanstack/react-query";
import { useEffect, useId, useState } from "react";
import { Link } from "react-router";
import { cardRuns, history, script } from "../../api/calls/board";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Board, Card, InboxItem } from "../../api/types/board";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { KanbanBadge } from "../../ui/components/KanbanBadge";
import { KanbanIssuePanel, KanbanIssueSection } from "../../ui/components/KanbanIssuePanel";
import { RunningDots } from "../../ui/components/RunningDots";
import type { Decision } from "./decisions";
import {
  ago,
  type CardStatus,
  cardStatus,
  describeHistory,
  issueUrl,
  STATUS_LABELS,
  tokens,
} from "./model";

const STATUS_TONE: Record<CardStatus, "plain" | "live" | "teal" | "amber" | "red"> = {
  live: "live",
  "needs-you": "amber",
  stuck: "red",
  blocked: "red",
  waiting: "amber",
  idle: "plain",
};

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex items-baseline gap-base border-b border-line/50 py-half last:border-b-0">
      <dt className="w-14 shrink-0 font-mono text-[11px] text-low">{label}</dt>
      <dd className="m-0 min-w-0 flex-1 text-xs text-normal [overflow-wrap:anywhere]">
        {children}
      </dd>
    </div>
  );
}

/** What the card waits for from a person, as buttons; sending a change back asks why first. */
function Actions({
  card,
  item,
  busy,
  rejectAsk,
  onDecide,
}: {
  card: Card;
  item: InboxItem;
  busy: boolean;
  rejectAsk: number;
  onDecide: (decision: Decision) => void;
}) {
  const [rejecting, setRejecting] = useState(false);
  const [reason, setReason] = useState("");
  const reasonId = useId();
  useEffect(() => {
    if (rejectAsk > 0) setRejecting(true);
  }, [rejectAsk]);
  const change = typeof item.data.change === "number" ? item.data.change : null;
  const number = card.number;

  if (rejecting && change !== null) {
    const ready = reason.trim() !== "";
    const send = () => {
      if (!ready || busy) return;
      onDecide({ kind: "reject", change, number, reason: reason.trim() });
      setRejecting(false);
      setReason("");
    };
    return (
      <form
        className="flex flex-col gap-base"
        onSubmit={(e) => {
          e.preventDefault();
          send();
        }}
      >
        <label htmlFor={reasonId} className="text-xs text-low">
          Why? The next round is told
        </label>
        <Input id={reasonId} value={reason} onChange={(e) => setReason(e.target.value)} autoFocus />
        <div className="flex gap-base">
          <Button type="submit" size="sm" disabled={!ready || busy}>
            Send back
          </Button>
          <Button type="button" size="sm" variant="ghost" onClick={() => setRejecting(false)}>
            Cancel
          </Button>
        </div>
      </form>
    );
  }

  return (
    <div className="flex flex-wrap gap-base">
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
          <Button size="sm" variant="outline" disabled={busy} onClick={() => setRejecting(true)}>
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
  if (query.isPending) return <p className="m-0 text-xs text-low">Reading…</p>;
  if (query.error) {
    return (
      <p role="alert" className="m-0 text-xs text-error">
        {isApiError(query.error) ? query.error.message : "Could not read this."}
      </p>
    );
  }
  return empty ? <p className="m-0 text-xs text-low">{empty}</p> : null;
}

export function Dock({
  board,
  card,
  item,
  busy,
  rejectAsk,
  onDecide,
  onClose,
}: {
  board: Board;
  card: Card;
  item: InboxItem | undefined;
  busy: boolean;
  /** Bumped when a drop asks to send the change back; the reason form opens. */
  rejectAsk: number;
  onDecide: (decision: Decision) => void;
  onClose: () => void;
}) {
  const repository = board.repository;
  const status = cardStatus(card, item, board);
  const runs = useQuery({ queryKey: keys.cardRuns(card.id), queryFn: () => cardRuns(card.id) });
  const entries = useQuery({ queryKey: keys.history(card.id), queryFn: () => history(card.id) });
  const source = useQuery({
    queryKey: keys.script(repository.id, card.column),
    queryFn: () => script(repository.id, card.column),
  });
  const now = Math.floor(Date.now() / 1000);
  const wait = board.waiting[String(card.id)];
  const state = card.change
    ? `checks: ${card.change.checks}`
    : card.blocked_by.length > 0
      ? `blocked by ${card.blocked_by.map((n) => `#${n}`).join(", ")}`
      : `tokens: ${tokens(card.tokens)}`;

  return (
    <KanbanIssuePanel
      displayId={`#${card.number}`}
      issueUrl={issueUrl(repository, card)}
      title={card.title}
      status={<KanbanBadge name={STATUS_LABELS[status]} tone={STATUS_TONE[status]} />}
      onClose={onClose}
    >
      <div className="px-base pb-base font-mono text-[11px] text-low">
        {repository.name} · {card.column} · by {card.author}
      </div>

      {item && (
        <KanbanIssueSection title="Waits for you">
          <p className="m-0 mb-base text-xs text-normal [overflow-wrap:anywhere]">{item.title}</p>
          <Actions card={card} item={item} busy={busy} rejectAsk={rejectAsk} onDecide={onDecide} />
        </KanbanIssueSection>
      )}

      <KanbanIssueSection title="Where it stands">
        <dl className="m-0">
          <Row label="run">
            {card.run ? (
              <span className="inline-flex items-center gap-base">
                {card.run.agent} · {card.run.purpose} ({card.run.status})
                {status === "live" && <RunningDots />}
              </span>
            ) : (
              "no run started yet"
            )}
          </Row>
          <Row label="guard">
            {wait ?? "Ready in column"}
            {card.refusals > 0 &&
              ` · ${card.refusals} ${card.refusals === 1 ? "rule" : "rules"} broken`}
          </Row>
          <Row label="state">
            {state} · {tokens(card.tokens)}
          </Row>
          {card.note && <Row label="note">{card.note}</Row>}
        </dl>
        {card.run && (
          <Link
            to={`/runs/${card.run.id}`}
            className="mt-base inline-block text-xs text-accent hover:underline"
          >
            Open live view
          </Link>
        )}
      </KanbanIssueSection>

      <KanbanIssueSection title="Runs">
        <QueryText query={runs} empty={runs.data?.length === 0 ? "No runs yet." : undefined} />
        <ul className="m-0 list-none p-0">
          {(runs.data ?? []).map((run) => (
            <li key={run.id} className="py-[2px] text-xs">
              <Link to={`/runs/${run.id}`} className="text-accent hover:underline">
                Run {run.id}
              </Link>
              <span className="text-normal">
                {" "}
                · {run.agent} · {run.purpose} · {run.status}
              </span>
            </li>
          ))}
        </ul>
      </KanbanIssueSection>

      <KanbanIssueSection title="History">
        <QueryText
          query={entries}
          empty={entries.data?.length === 0 ? "No history yet." : undefined}
        />
        <ol className="m-0 list-none p-0">
          {(entries.data ?? []).map((entry, index) => (
            // The entries have no id; they are shown in the order the server sends them.
            // biome-ignore lint/suspicious/noArrayIndexKey: the history is a fixed, ordered list
            <li key={index} className="flex gap-base py-[2px] text-xs">
              <span className="w-16 shrink-0 font-mono text-[11px] text-low">
                {ago(now, entry.at)}
              </span>
              <span className="min-w-0 flex-1 text-normal [overflow-wrap:anywhere]">
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
            <div className="mb-half font-mono text-[11px] text-low">
              {source.data.path} ·{" "}
              {source.data.source === "repository" ? "this repository's own" : "built in"}
            </div>
            <pre
              data-testid="script"
              className="m-0 max-h-48 overflow-auto rounded-md border bg-recessed p-base font-mono text-[11px] text-normal"
            >
              {source.data.text}
            </pre>
            <Link
              to={`/board/${repository.id}/scripts/${encodeURIComponent(card.column)}`}
              className="mt-base inline-block text-xs text-accent hover:underline"
            >
              Open the script
            </Link>
          </>
        )}
      </KanbanIssueSection>
    </KanbanIssuePanel>
  );
}
