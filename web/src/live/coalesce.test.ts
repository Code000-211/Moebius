import { QueryClient, QueryObserver } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { keys } from "../api/keys";
import { COALESCE_MS } from "./coalesce";
import { applyEvent } from "./invalidation";

function observed() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const reads = { snapshot: 0 };
  const release: Array<() => void> = [];
  return { queryClient, reads, release };
}

const moved = { kind: "card_moved", data: { card: 1, from: "a", to: "b" } };

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

async function mountSnapshot(ctx: ReturnType<typeof observed>) {
  const observer = new QueryObserver(ctx.queryClient, {
    queryKey: keys.snapshot,
    queryFn: async () => {
      ctx.reads.snapshot += 1;
      if (hold(ctx)) await new Promise<void>((resolve) => ctx.release.push(resolve));
      return { boards: [], harness_changes: [], inbox: [], n: ctx.reads.snapshot };
    },
  });
  const unsubscribe = observer.subscribe(() => {});
  await vi.advanceTimersByTimeAsync(0);
  return unsubscribe;
}
const holds = new WeakMap<object, boolean>();
const hold = (ctx: object) => holds.get(ctx) ?? false;

describe("coalescing", () => {
  it("reads a key once for a burst of events and marks it stale at once", async () => {
    const ctx = observed();
    const stop = await mountSnapshot(ctx);
    expect(ctx.reads.snapshot).toBe(1);
    for (let i = 0; i < 50; i++) applyEvent(ctx.queryClient, moved);
    expect(ctx.queryClient.getQueryState(keys.snapshot)?.isInvalidated).toBe(true);
    expect(ctx.reads.snapshot).toBe(1);
    await vi.advanceTimersByTimeAsync(COALESCE_MS);
    expect(ctx.reads.snapshot).toBe(2);
    await vi.advanceTimersByTimeAsync(COALESCE_MS * 3);
    expect(ctx.reads.snapshot).toBe(2);
    stop();
  });

  it("reads at most once a window under a steady stream", async () => {
    const ctx = observed();
    const stop = await mountSnapshot(ctx);
    for (let i = 0; i < 100; i++) {
      applyEvent(ctx.queryClient, moved);
      await vi.advanceTimersByTimeAsync(100);
    }
    // 10 s of events every 100 ms: about one read per second, plus the first.
    expect(ctx.reads.snapshot).toBeLessThanOrEqual(11);
    expect(ctx.reads.snapshot).toBeGreaterThanOrEqual(9);
    stop();
  });

  it("does not cancel a read in flight and reads once more when it ends", async () => {
    const ctx = observed();
    const stop = await mountSnapshot(ctx);
    holds.set(ctx, true);
    applyEvent(ctx.queryClient, moved);
    await vi.advanceTimersByTimeAsync(COALESCE_MS);
    expect(ctx.reads.snapshot).toBe(2); // in flight
    applyEvent(ctx.queryClient, moved);
    await vi.advanceTimersByTimeAsync(COALESCE_MS);
    expect(ctx.reads.snapshot).toBe(2); // not started again, not cancelled
    holds.set(ctx, false);
    for (const release of ctx.release.splice(0)) release();
    await vi.advanceTimersByTimeAsync(COALESCE_MS);
    expect(ctx.reads.snapshot).toBe(3);
    stop();
  });

  it("does not read a query nobody shows, only marks it stale", async () => {
    const ctx = observed();
    ctx.queryClient.setQueryData(keys.snapshot, {});
    applyEvent(ctx.queryClient, moved);
    await vi.advanceTimersByTimeAsync(COALESCE_MS);
    expect(ctx.queryClient.getQueryState(keys.snapshot)?.isInvalidated).toBe(true);
    expect(ctx.reads.snapshot).toBe(0);
  });

  it("does not make the snapshot stale for card_waiting, which is patched in place", async () => {
    const ctx = observed();
    const stop = await mountSnapshot(ctx);
    applyEvent(ctx.queryClient, { kind: "card_waiting", data: { card: 7, reason: "x" } });
    await vi.advanceTimersByTimeAsync(COALESCE_MS * 2);
    expect(ctx.queryClient.getQueryState(keys.snapshot)?.isInvalidated).toBe(false);
    expect(ctx.reads.snapshot).toBe(1);
    stop();
  });
});
