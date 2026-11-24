/**
 * The one tinted block at the end of a card's thread: "Moebius needs you", what the card waits for (the item of the
 * Inbox), and the buttons for it — approve the plan, merge, send back with a reason, retry — and "Reply", which takes
 * the person to the reply field. It is the card window's `Actions` of old, drawn in the thread where the question was
 * asked; the decision itself is still made by the page (`decisions.ts`).
 */
import { useEffect, useState } from "react";
import type { Card, InboxItem } from "../../api/types/board";
import { Button } from "../../ui/components/Button";
import { ago } from "../../ui/lib/format";
import { useNow } from "../../ui/lib/useNow";
import type { Decision } from "./decisions";
import {
  DONE_ASKED,
  OUTSIDE_CHANGE,
  REVIEW_NOT_POSSIBLE,
  REVIEW_ROUNDS_SPENT,
} from "./model/inbox";

/** The mark of Moebius in the thread: a dark square with its initial. */
export function MoebiusMark() {
  return (
    <span
      aria-hidden="true"
      className="inline-flex size-6 shrink-0 items-center justify-center rounded-control bg-avatar-agent text-xs font-semibold leading-none text-on-accent"
    >
      M
    </span>
  );
}

/** The login that wrote `/done`, and what else they wrote, as the item keeps them. */
const loginOf = (item: InboxItem): string =>
  typeof item.data.login === "string" ? item.data.login : "the assignee";
const reasonOf = (item: InboxItem): string =>
  typeof item.data.reason === "string" ? item.data.reason : "";

export function NeedsYouBlock({
  card,
  item,
  busy,
  askFor,
  onAskAnswered,
  onDecide,
  onReply,
}: {
  card: Card;
  item: InboxItem;
  busy: boolean;
  /** The card a send-back is waiting on a reason for; this one, or none. */
  askFor: number | null;
  /** Says that the reason was sent or dropped, so the next send-back asks again. */
  onAskAnswered: () => void;
  onDecide: (decision: Decision) => void;
  /** Takes the person to the reply field. */
  onReply: () => void;
}) {
  const now = useNow();
  const [rejecting, setRejecting] = useState(false);
  const [reason, setReason] = useState("");
  // The ask names the card it is about, so opening another card does not find this one's question waiting for it.
  useEffect(() => {
    if (askFor === card.id) setRejecting(true);
  }, [askFor, card.id]);
  const change = typeof item.data.change === "number" ? item.data.change : null;
  // The pull request of someone else: both items offer its choices.
  const outside =
    item.kind === OUTSIDE_CHANGE ||
    item.kind === REVIEW_ROUNDS_SPENT ||
    item.kind === REVIEW_NOT_POSSIBLE;
  // A review that asked for changes is the author's turn until they push: there is nothing to merge yet.
  const review = card.change?.external_review;
  const waitingForAuthor =
    outside && review?.verdict === "changes" && review.state === "posted" && review.current;
  const author = card.change?.outside_author;
  const number = card.number;
  const ready = reason.trim() !== "";

  const send = () => {
    if (!ready || busy || change === null) return;
    onDecide({ kind: "reject", change, number, reason: reason.trim() });
    setRejecting(false);
    setReason("");
    onAskAnswered();
  };

  return (
    <div
      data-testid="needs-you-block"
      className="flex min-w-0 flex-col gap-3 rounded-panel border border-needs-you-edge bg-needs-you-tint px-4 py-3"
    >
      <div className="flex items-baseline justify-between gap-3">
        <span className="text-md font-semibold text-text">Moebius needs you</span>
        <span className="shrink-0 text-sm tabular-nums text-secondary">{ago(now, item.at)}</span>
      </div>
      <p className="m-0 text-md text-text [overflow-wrap:anywhere]">{item.title}</p>
      {waitingForAuthor && (
        <p data-testid="waiting-for-author" className="m-0 text-base text-secondary">
          {`Waiting for @${author ?? "the author"} to push changes after the review. Merge is back once they do.`}
        </p>
      )}
      {item.kind === DONE_ASKED && reasonOf(item) !== "" && (
        <p
          data-testid="done-reason"
          className="m-0 whitespace-pre-line border-l-2 border-hairline-strong pl-3 text-base text-secondary [overflow-wrap:anywhere]"
        >
          {reasonOf(item)}
        </p>
      )}
      {rejecting && change !== null ? (
        <form
          className="flex flex-wrap items-center gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            send();
          }}
        >
          <input
            aria-label="Why? The next round is told"
            placeholder="Why? The next round is told"
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            // biome-ignore lint/a11y/noAutofocus: the person just asked to send the change back, and the reason is all that is left to say
            autoFocus
            className="min-h-8 min-w-48 flex-1 rounded-control border border-hairline-strong bg-panel px-3 text-base text-text placeholder:text-muted"
          />
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
        </form>
      ) : (
        <div className="flex flex-wrap gap-2">
          {item.kind === "approve_issue" && (
            <Button
              disabled={busy}
              onClick={() => onDecide({ kind: "approve", card: card.id, number })}
            >
              Approve
            </Button>
          )}
          {item.kind === "ready_to_merge" && change !== null && (
            <>
              <Button disabled={busy} onClick={() => onDecide({ kind: "merge", change, number })}>
                Merge
              </Button>
              <Button variant="secondary" disabled={busy} onClick={() => setRejecting(true)}>
                Reject with a reason
              </Button>
            </>
          )}
          {outside && change !== null && (
            <>
              <Button
                variant="secondary"
                disabled={busy}
                onClick={() => onDecide({ kind: "review", change, number })}
              >
                Review with agent
              </Button>
              {!waitingForAuthor && (
                <Button
                  disabled={
                    busy || card.change?.checks === "pending" || card.change?.checks === "failure"
                  }
                  title={
                    card.change?.checks === "pending" || card.change?.checks === "failure"
                      ? "Merge once the checks have passed"
                      : undefined
                  }
                  onClick={() => onDecide({ kind: "merge", change, number })}
                >
                  Merge
                </Button>
              )}
            </>
          )}
          {item.kind === DONE_ASKED && (
            <>
              <Button
                disabled={busy}
                onClick={() => onDecide({ kind: "done", card: card.id, number, answer: "close" })}
              >
                Close the issue
              </Button>
              <Button
                variant="secondary"
                disabled={busy}
                onClick={() =>
                  onDecide({ kind: "done", card: card.id, number, answer: "give_back" })
                }
              >
                {`Give it back to @${loginOf(item)}`}
              </Button>
              <Button
                variant="secondary"
                disabled={busy}
                onClick={() => onDecide({ kind: "done", card: card.id, number, answer: "release" })}
              >
                Release it
              </Button>
            </>
          )}
          {(item.kind === "stuck" || item.kind === "change_closed") && (
            <Button
              disabled={busy}
              onClick={() => onDecide({ kind: "retry", card: card.id, number })}
            >
              Retry
            </Button>
          )}
          <Button variant="secondary" onClick={onReply}>
            Reply
          </Button>
        </div>
      )}
    </div>
  );
}
