/**
 * What the server's events do to the TanStack Query cache. The reference says the snapshot is read again on every
 * event, so an event that changes what the snapshot shows invalidates it; `card_waiting` is also patched in place,
 * which shows the reason at once. What is read about one card (history, runs) goes stale with the snapshot, and the
 * column scripts with `script_changed`. A new page that reads something the events change adds its key to `keys.ts` and a
 * line to `invalidates` below. Events are not replayed, so when the socket connects all that is shown is read again.
 */
import type { QueryClient, QueryKey } from "@tanstack/react-query";
import { keys } from "../api/keys";
import type { Snapshot } from "../api/types/board";
import type { ServerEvent } from "../api/types/events";

/** The kinds that change what the snapshot holds: repositories, cards, changes, runs of cards and the inbox. */
const SNAPSHOT_KINDS = new Set([
  "repository_added",
  "repository_updated",
  "repository_removed",
  "card_added",
  "card_updated",
  "card_moved",
  "card_waiting",
  "change_offered",
  "change_updated",
  "change_merged",
  "change_closed",
  "attempt_judged",
  "attempts_settled",
  "run_started",
  "run_ended",
  "grilling_ended",
  "proposal_status",
  "takeover_proposed",
  "takeover_status",
  "retro_done",
]);

/** The keys of the cache that `event` makes stale. `lagged` means events were lost, so everything is. */
export function invalidates(event: ServerEvent): QueryKey[] {
  if (event.kind === "lagged") return [[]];
  const stale: QueryKey[] = [];
  if (SNAPSHOT_KINDS.has(event.kind)) stale.push(keys.snapshot, keys.cards);
  if (event.kind === "script_changed") stale.push(keys.scripts);
  if (event.kind === "repository_added" || event.kind === "repository_removed") {
    stale.push(keys.start);
  }
  return stale;
}

/** Puts the reason of `card_waiting` into the snapshot in the cache, if one is there. */
export function patchSnapshot(queryClient: QueryClient, event: ServerEvent): void {
  if (event.kind !== "card_waiting") return;
  const card = event.data.card;
  const reason = event.data.reason;
  if (typeof card !== "number") return;
  queryClient.setQueryData<Snapshot>(keys.snapshot, (snapshot) => {
    if (!snapshot) return snapshot;
    return {
      ...snapshot,
      boards: snapshot.boards.map((board) => {
        if (!board.cards.some((c) => c.id === card)) return board;
        const waiting = { ...board.waiting };
        if (typeof reason === "string") waiting[String(card)] = reason;
        else delete waiting[String(card)];
        return { ...board, waiting };
      }),
    };
  });
}

/** Applies one event to the cache. */
export function applyEvent(queryClient: QueryClient, event: ServerEvent): void {
  patchSnapshot(queryClient, event);
  for (const queryKey of invalidates(event)) {
    void queryClient.invalidateQueries({ queryKey });
  }
}

/** The socket connected (again): what is shown may have missed events, so every read is made again. */
export function applyConnected(queryClient: QueryClient): void {
  void queryClient.invalidateQueries({ predicate: (query) => query.queryKey[0] !== "session" });
}
