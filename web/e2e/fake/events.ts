/**
 * How `GET /v1/runs/{id}/events` answers, for the parts of the fake that store a run's events (e2e/fake/chat.ts and
 * e2e/fake/runs.ts): forwards from a `seq`, or backwards — from the end with `tail`, or below a `seq` with `before`.
 * A page read backwards comes back oldest first, like any other, because a chat is read from its beginning whichever
 * way the reader came.
 */
export interface PagedEvent {
  seq: number;
}

/** One page of `stored`, chosen by the query of the request. */
export function eventPage<T extends PagedEvent>(stored: readonly T[], query: URLSearchParams): T[] {
  const limit = Number(query.get("limit") ?? 500);
  const before = query.get("before");
  if (query.get("tail") !== null || before !== null) {
    const below = before === null ? Number.MAX_SAFE_INTEGER : Number(before);
    return stored.filter((event) => event.seq < below).slice(-limit);
  }
  const after = Number(query.get("after") ?? 0);
  return stored.filter((event) => event.seq > after).slice(0, limit);
}
