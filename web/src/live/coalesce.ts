/**
 * Spaces out the reads that events cause. An event marks the queries it makes stale at once (what is on screen is
 * known to be old, and a query that is not on screen is read when it is opened), but the queries on screen are read
 * again only when a short window after the first event of a burst has passed, one read per key however many events
 * came. A read in flight is never cancelled by a newer event; when it ends, the key is read once more, because
 * the answer may have been made before the event.
 */
import type { QueryClient, QueryKey } from "@tanstack/react-query";

/** How long events are collected before the queries on screen are read again. */
export const COALESCE_MS = 1000;

class Coalescer {
  private pending = new Map<string, QueryKey>();
  private timer: ReturnType<typeof setTimeout> | null = null;

  constructor(private readonly queryClient: QueryClient) {}

  /** Makes the queries under each key stale now and reads the ones on screen at the end of the window. */
  mark(queryKeys: QueryKey[]): void {
    if (queryKeys.length === 0) return;
    for (const queryKey of queryKeys) {
      for (const query of this.queryClient.getQueryCache().findAll({ queryKey }))
        query.invalidate();
      this.pending.set(JSON.stringify(queryKey), queryKey);
    }
    this.schedule();
  }

  private schedule(): void {
    if (this.timer !== null) return;
    this.timer = setTimeout(() => {
      this.timer = null;
      this.flush();
    }, COALESCE_MS);
  }

  private flush(): void {
    const queryKeys = [...this.pending.values()];
    this.pending.clear();
    for (const queryKey of queryKeys) {
      // A read that is running began before some of these events: it is let finish, then the key is read again.
      const running = this.queryClient.isFetching({ queryKey }) > 0;
      void this.queryClient.invalidateQueries({ queryKey }, { cancelRefetch: false }).then(() => {
        if (!running) return;
        this.pending.set(JSON.stringify(queryKey), queryKey);
        this.schedule();
      });
    }
  }
}

const coalescers = new WeakMap<QueryClient, Coalescer>();

/** Marks `queryKeys` stale in `queryClient` now and reads them again, coalesced with the other events of the window. */
export function markStale(queryClient: QueryClient, queryKeys: QueryKey[]): void {
  let coalescer = coalescers.get(queryClient);
  if (!coalescer) {
    coalescer = new Coalescer(queryClient);
    coalescers.set(queryClient, coalescer);
  }
  coalescer.mark(queryKeys);
}
