/**
 * What a person decides about a card: approve an issue, merge a change, send it back, retry a stuck card, unblock a
 * blocked one, stop a card's live run (blocking and deleting ask a question first, in `CardDialogs.tsx`). One hook
 * sends the request, tells the outcome in a toast (what the server did, or why it refused) and reads the board
 * again; the events of the server do the same, so a second client sees it too.
 */
import { type QueryClient, useMutation, useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { approve, merge, reject, retry, unblock } from "../../api/calls/board";
import { stopRun } from "../../api/calls/chat";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import { putCards } from "../../live/cardCache";
import { useToast } from "../../ui/page/Toast";

export type Decision =
  | { kind: "approve"; card: number; number: number }
  | { kind: "retry"; card: number; number: number }
  | { kind: "unblock"; card: number; number: number }
  | { kind: "stop"; run: number; number: number }
  | { kind: "merge"; change: number; number: number }
  | { kind: "reject"; change: number; number: number; reason: string };

/** What the page tells after a decision or a refused drop: a toast, with Undo when it can be taken back. */
export interface Notice {
  tone: "ok" | "error";
  text: string;
  undo?: () => void;
}

/** Shows a notice as a toast. */
export function useNotify(): (notice: Notice) => void {
  const toast = useToast();
  return useCallback(
    ({ tone, text, undo }: Notice) =>
      toast({ kind: tone === "error" ? "error" : "confirmation", text, undo }),
    [toast],
  );
}

const VERB: Record<Decision["kind"], string> = {
  approve: "approve",
  retry: "retry",
  unblock: "unblock",
  stop: "stop",
  merge: "merge",
  reject: "send back",
};

async function send(decision: Decision, queryClient: QueryClient): Promise<string> {
  const n = `#${decision.number}`;
  switch (decision.kind) {
    case "approve":
      putCards(queryClient, [await approve(decision.card)]);
      return `Approved ${n}. It moves to Not started.`;
    case "retry":
      putCards(queryClient, [await retry(decision.card)]);
      return `Retrying ${n}.`;
    case "unblock":
      putCards(queryClient, [await unblock(decision.card)]);
      return `Unblocked ${n}. The board works on it again.`;
    case "stop":
      await stopRun(decision.run);
      return `Stopping ${n}. It waits for you in the inbox once its agent has settled.`;
    case "merge": {
      const { commit } = await merge(decision.change);
      return `Merged ${n}. Commit ${commit.slice(0, 7)}.`;
    }
    case "reject":
      putCards(queryClient, [await reject(decision.change, decision.reason)]);
      return `Sent ${n} back to coding.`;
  }
}

/** The sentence for a failed request: what could not be done and the server's reason. */
export function failure(decision: Decision, error: unknown): string {
  const reason = isApiError(error) ? error.message : "something went wrong";
  return `Could not ${VERB[decision.kind]} #${decision.number}: ${reason}`;
}

export function useDecisions() {
  const queryClient = useQueryClient();
  const notify = useNotify();
  const mutation = useMutation({
    mutationFn: (decision: Decision) => send(decision, queryClient),
    onSuccess: (text) => notify({ tone: "ok", text }),
    onError: (error, decision) => {
      // Refused: read the board, so what is shown is what the server has.
      void queryClient.invalidateQueries({ queryKey: keys.snapshot });
      void queryClient.invalidateQueries({ queryKey: keys.cards });
      notify({ tone: "error", text: failure(decision, error) });
    },
  });
  // `mutate` keeps its identity from one render to the next, which the board's memoised cards rely on.
  const { mutate } = mutation;
  const decide = useCallback((decision: Decision) => mutate(decision), [mutate]);
  return { decide, pending: mutation.isPending, notify };
}
