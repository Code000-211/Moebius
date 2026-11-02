// @vitest-environment jsdom
/**
 * The follow of one chat: reading it when it opens, and keeping it whole when the stream cannot deliver everything.
 * Two things go wrong there and both were invisible: a page opened while the socket was trying again read nothing at
 * all and never marked the chat as read, and a client the server dropped events rather than queued was left with a
 * hole in the middle of its `seq`s — part of the answer missing, and an unread marker that could never be satisfied.
 * Both are answered by reading again, so that is what these tests watch for.
 */
import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { RunEvent } from "../api/types/chat";

const live = vi.hoisted(() => ({
  connection: "online" as string,
  listener: null as ((event: { kind: string; data: Record<string, unknown> }) => void) | null,
}));
const api = vi.hoisted(() => ({
  /** What `GET /v1/runs/{id}/events` answers, each time it is asked. */
  pages: [] as (RunEvent[] | Error | Promise<RunEvent[]>)[],
  /** The `after` of every read, in the order they were asked for. */
  asked: [] as number[],
}));

vi.mock("../live/LiveProvider", () => ({
  useConnection: () => live.connection,
  useServerEvents: (listener: (event: { kind: string; data: Record<string, unknown> }) => void) => {
    live.listener = listener;
  },
}));

vi.mock("../api/calls/chat", () => ({
  runEvents: async (_run: number, after: number) => {
    api.asked.push(after);
    const page = api.pages[api.asked.length - 1] ?? [];
    if (page instanceof Error) throw page;
    return page;
  },
}));

const { useThread } = await import("./useThread");

/** A piece of an answer, as the server stores it: the `seq` is what orders the chat, `at` is the server's clock. */
function event(seq: number, text = `piece ${seq}`, at = 1_700_000_000 + seq): RunEvent {
  return {
    seq,
    kind: "message_update",
    data: { assistantMessageEvent: { type: "text_delta", contentIndex: 0, delta: text } },
    at,
  };
}

beforeEach(() => {
  live.connection = "online";
  live.listener = null;
  api.pages = [];
  api.asked = [];
});

describe("following a chat", () => {
  it("reads it even while the stream is still trying to connect", async () => {
    // A laptop wakes, or the server is restarted: the socket is behind, but reading is an ordinary request and the
    // chat is there to be read. It used to be left on "Reading the chat", unread, until the socket opened again.
    live.connection = "reconnecting";
    api.pages = [[event(1), event(2)]];
    const { result } = renderHook(() => useThread(7));
    await waitFor(() => expect(result.current.loaded).toBe(true));
    expect(result.current.entries.at(-1)).toMatchObject({ kind: "text", text: "piece 1piece 2" });
    expect(result.current.lastSeq).toBe(2);
    expect(api.asked).toEqual([0]);
  });

  it("reads again when the server says it dropped events, and closes the hole", async () => {
    api.pages = [[event(1)], [event(2), event(3)]];
    const { result } = renderHook(() => useThread(7));
    await waitFor(() => expect(result.current.lastSeq).toBe(1));
    // The burst of a fast answer overflowed the queue: the stream will never bring 2 and 3, and only a read will.
    act(() => live.listener?.({ kind: "lagged", data: { missed: 2 } }));
    await waitFor(() => expect(result.current.lastSeq).toBe(3));
    expect(api.asked).toEqual([0, 1]);
    // Everything the stream lost is there again, in the order the `seq`s give it.
    expect(result.current.entries.at(-1)).toMatchObject({
      kind: "text",
      text: "piece 1piece 2piece 3",
    });
  });

  it("ignores an event of another chat and one of no chat at all", async () => {
    api.pages = [[event(1)]];
    const { result } = renderHook(() => useThread(7));
    await waitFor(() => expect(result.current.lastSeq).toBe(1));
    act(() => live.listener?.({ kind: "lagged", data: { missed: 1 } }));
    await waitFor(() => expect(api.asked).toHaveLength(2));
    // A `run_event` of another chat changes nothing about this one.
    act(() =>
      live.listener?.({
        kind: "run_event",
        data: { run: 8, seq: 9, kind: "message_update", data: {} },
      }),
    );
    expect(result.current.lastSeq).toBe(1);
    expect(result.current.entries.at(-1)).toMatchObject({ text: "piece 1" });
  });

  it("gives up the chat when the page closes it", async () => {
    api.pages = [[event(1)]];
    const { result, rerender } = renderHook(({ id }: { id: number | null }) => useThread(id), {
      initialProps: { id: 7 as number | null },
    });
    await waitFor(() => expect(result.current.lastSeq).toBe(1));
    rerender({ id: null });
    expect(result.current).toEqual({ entries: [], lastSeq: 0, loaded: false, error: null });
  });

  it("keeps a chat on screen when reading it again fails while the stream reconnects", async () => {
    api.pages = [[event(1), event(2)], new Error("cannot reach the server")];
    const { result, rerender } = renderHook(() => useThread(7));
    await waitFor(() => expect(result.current.loaded).toBe(true));
    live.connection = "reconnecting";
    rerender();
    await waitFor(() => expect(result.current.error).toBe("cannot reach the server"));
    // The conversation stays; the page shows the reason above it.
    expect(result.current.loaded).toBe(true);
    expect(result.current.entries.at(-1)).toMatchObject({ text: "piece 1piece 2" });
  });

  it("does not let an older failed read overwrite a newer successful one", async () => {
    let fail: (e: Error) => void = () => {};
    const slow = new Promise<RunEvent[]>((_, reject) => {
      fail = reject;
    });
    live.connection = "reconnecting";
    api.pages = [slow, [event(1)]];
    const { result, rerender } = renderHook(() => useThread(7));
    await waitFor(() => expect(api.asked).toHaveLength(1));
    live.connection = "online";
    rerender();
    await waitFor(() => expect(result.current.loaded).toBe(true));
    await act(async () => {
      fail(new Error("cannot reach the server"));
      await slow.catch(() => {});
    });
    expect(result.current.error).toBeNull();
    expect(result.current.entries.at(-1)).toMatchObject({ text: "piece 1" });
  });
});
