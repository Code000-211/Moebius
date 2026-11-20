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
              size="sm"
              disabled={busy}
              onClick={() => onDecide({ kind: "approve", card: card.id, number })}
            >
              Approve plan
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
              <Button
                size="sm"
                variant="secondary"
                disabled={busy}
                onClick={() => setRejecting(true)}
              >
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
          <Button size="sm" variant="secondary" onClick={onReply}>
            Reply
          </Button>
        </div>
      )}
    </div>
  );
}
