/**
 * What a person decides about a card: approve an issue, merge a change, send it back, retry a stuck card. One hook
 * sends the request, tells the outcome in a sentence (what the server did, or why it refused) and reads the board
 * again; the events of the server do the same, so a second client sees it too.
 */
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useCallback, useState } from "react";
import { approve, merge, reject, retry } from "../../api/calls/board";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";

export type Decision =
  | { kind: "approve"; card: number; number: number }
  | { kind: "retry"; card: number; number: number }
  | { kind: "merge"; change: number; number: number }
  | { kind: "reject"; change: number; number: number; reason: string };

/** What the page tells after a decision or a refused drop. */
export interface Notice {
  tone: "ok" | "error";
  text: string;
}

const VERB: Record<Decision["kind"], string> = {
  approve: "approve",
  retry: "retry",
  merge: "merge",
  reject: "send back",
};

async function send(decision: Decision): Promise<string> {
  const n = `#${decision.number}`;
  switch (decision.kind) {
    case "approve":
      await approve(decision.card);
      return `Approved ${n}. It moves to Ready.`;
    case "retry":
      await retry(decision.card);
      return `Retrying ${n}.`;
    case "merge": {
      const { commit } = await merge(decision.change);
      return `Merged ${n}. Commit ${commit.slice(0, 7)}.`;
    }
    case "reject":
      await reject(decision.change, decision.reason);
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
  const [notice, setNotice] = useState<Notice | null>(null);
  const mutation = useMutation({
    mutationFn: send,
    onSuccess: (text) => setNotice({ tone: "ok", text }),
    onError: (error, decision) => setNotice({ tone: "error", text: failure(decision, error) }),
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: keys.snapshot });
      void queryClient.invalidateQueries({ queryKey: keys.cards });
    },
  });
  const decide = useCallback((decision: Decision) => mutation.mutate(decision), [mutation]);
  return { decide, pending: mutation.isPending, notice, setNotice };
}
