// @vitest-environment jsdom
/**
 * The follow of one chat: reading it when it opens, keeping it whole when the stream cannot deliver everything, and
 * leaving the part above the window for a person to ask for. Three things go wrong there and none was visible: a page
 * opened while the socket was trying again read nothing at all and never marked the chat as read; a client the server
 * dropped events rather than queued was left with a hole in its `seq`s — part of the answer missing, and an unread
 * marker that could never be satisfied; and a long chat was read from its first event, so the page waited for
 * thousands of events nobody was going to scroll up to.
 */
import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { RunEvent } from "../api/types/chat";

/** The size of one page of events, as small as the hook is told they are. */
const PAGE = 100;

const live = vi.hoisted(() => ({
  connection: "online" as string,
  listener: null as ((event: { kind: string; data: Record<string, unknown> }) => void) | null,
}));
const api = vi.hoisted(() => ({
  /** What the backwards reads answer, the end of the chat first. */
  tails: [] as (RunEvent[] | Error | Promise<RunEvent[]>)[],
  /** What `GET /v1/runs/{id}/events` answers, each time it is asked. */
  pages: [] as (RunEvent[] | Error | Promise<RunEvent[]>)[],
  /** The `after` of every forward read, in the order they were asked for. */
  asked: [] as number[],
  /** The `before` of every backwards read; null is the end of the chat. */
  backwards: [] as (number | null)[],
}));

vi.mock("../live/LiveProvider", () => ({
  useConnection: () => live.connection,
  useServerEvents: (listener: (event: { kind: string; data: Record<string, unknown> }) => void) => {
    live.listener = listener;
  },
}));

vi.mock("../api/calls/chat", () => ({
  EVENT_PAGE: PAGE,
  runEvents: async (_run: number, after: number) => {
    api.asked.push(after);
    const page = api.pages[api.asked.length - 1] ?? [];
    if (page instanceof Error) throw page;
    return page;
  },
  olderEvents: async (_run: number, below: number | null) => {
    api.backwards.push(below);
    const page = api.tails[api.backwards.length - 1] ?? [];
    if (page instanceof Error) throw page;
    return page;
  },
}));

const { MAX_FIRST_PAGES, useThread } = await import("./useThread");

/** A piece of an answer, as the server stores it: the `seq` is what orders the chat, `at` is the server's clock. */
function event(seq: number, text = `piece ${seq}`, at = 1_700_000_000 + seq): RunEvent {
  return {
    seq,
    kind: "message_update",
    data: { assistantMessageEvent: { type: "text_delta", contentIndex: 0, delta: text } },
    at,
  };
}

/** A message of the person, which the conversation shows as an entry of its own. */
function said(seq: number): RunEvent {
  return { seq, kind: "said", data: { message: `go ${seq}` }, at: 1_700_000_000 + seq };
}

/** A page of `PAGE` pieces of one answer, which fold into a single entry however many there are. */
function pieces(from: number): RunEvent[] {
  return Array.from({ length: PAGE }, (_, place) => event(from + place));
}

/** A page as full as one comes, so the page knows there was more where it came from. */
function fullPage(from: number): RunEvent[] {
  return Array.from({ length: PAGE }, (_, place) => said(from + place));
}

beforeEach(() => {
  live.connection = "online";
  live.listener = null;
  api.tails = [];
  api.pages = [];
  api.asked = [];
  api.backwards = [];
});

describe("opening a chat", () => {
  it("reads it even while the stream is still trying to connect", async () => {
    // A laptop wakes, or the server is restarted: the socket is behind, but reading is an ordinary request and the
    // chat is there to be read. It used to be left on "Reading the chat", unread, until the socket opened again.
    live.connection = "reconnecting";
    api.tails = [[event(1), event(2)]];
    const { result } = renderHook(() => useThread(7));
    await waitFor(() => expect(result.current.loaded).toBe(true));
    expect(result.current.entries.at(-1)).toMatchObject({ kind: "text", text: "piece 1piece 2" });
    expect(result.current.lastSeq).toBe(2);
    expect(api.backwards).toEqual([null]);
  });

  it("stops at one page when its messages fill the window, and says there is more above", async () => {
    // A page full of events that are each a message of the person: the window is counted in messages, not events.
    api.tails = [fullPage(41)];
    const { result } = renderHook(() => useThread(7));
    await waitFor(() => expect(result.current.loaded).toBe(true));
    // One page of them, and the window stopped there rather than asking for the chat's beginning.
    expect(api.backwards).toEqual([null]);
    expect(result.current.hasOlder).toBe(true);
    expect(result.current.entries).toHaveLength(PAGE);
  });

  it("keeps reading pages until the window holds enough entries", async () => {
    // Full pages whose events fold into one entry: the window is counted in entries, so one page is not enough and
    // each one is asked for below the one before it, until a short page says the log has run out.
    api.tails = [pieces(201), pieces(101), [said(100)]];
    const { result } = renderHook(() => useThread(7));
    await waitFor(() => expect(result.current.loaded).toBe(true));
    expect(api.backwards).toEqual([null, 201, 101]);
    expect(result.current.hasOlder).toBe(false);
    expect(result.current.entries[0]).toMatchObject({ kind: "person", message: "go 100" });
  });

  it("stops after a number of pages and leaves the rest above the window", async () => {
    api.tails = Array.from({ length: MAX_FIRST_PAGES + 5 }, (_, place) =>
      pieces(10_000 - PAGE * (place + 1) + 1),
    );
    const { result } = renderHook(() => useThread(7));
    await waitFor(() => expect(result.current.loaded).toBe(true));
    expect(api.backwards).toHaveLength(MAX_FIRST_PAGES);
    expect(result.current.hasOlder).toBe(true);
  });

  it("asks for what is above the oldest event held, not the first one the stream brought", async () => {
    let answer: (events: RunEvent[]) => void = () => {};
    const slow = new Promise<RunEvent[]>((resolve) => {
      answer = resolve;
    });
    api.tails = [slow, [said(40)]];
    const { result } = renderHook(() => useThread(7));
    await waitFor(() => expect(api.backwards).toEqual([null]));
    // The stream brings an event while the end of the chat is still being read; it is newer than anything the read
    // will bring, and must not be taken for the oldest.
    act(() =>
      live.listener?.({
        kind: "run_event",
        data: { run: 7, seq: 200, kind: "said", data: { message: "live" } },
      }),
    );
    await act(async () => {
      answer(fullPage(41));
      await slow;
    });
    await waitFor(() => expect(result.current.hasOlder).toBe(true));
    act(() => result.current.loadOlder());
    await waitFor(() => expect(result.current.hasOlder).toBe(false));
    expect(api.backwards).toEqual([null, 41]);
  });

  it("reads the end of the chat again when the first read failed and the stream went on", async () => {
    api.tails = [new Error("cannot reach the server"), [said(3), said(4)]];
    api.pages = [[]];
    const { result, rerender } = renderHook(() => useThread(7));
    await waitFor(() => expect(result.current.error).toBe("cannot reach the server"));
    // The answer goes on while the history could not be read: the page still says it could not read it.
    act(() =>
      live.listener?.({
        kind: "run_event",
        data: { run: 7, seq: 5, kind: "said", data: { message: "live" } },
      }),
    );
    await waitFor(() => expect(result.current.lastSeq).toBe(5));
    expect(result.current.loaded).toBe(false);
    expect(result.current.error).toBe("cannot reach the server");
    // The socket connects again. Reading only after seq 5 would call the chat read with its history out of reach.
    live.connection = "reconnecting";
    rerender();
    await waitFor(() => expect(result.current.loaded).toBe(true));
    expect(api.backwards).toEqual([null, 5]);
    expect(result.current.entries.map((entry) => entry.seq)).toEqual([3, 4, 5]);
    expect(result.current.error).toBeNull();
    // And what the stream may have missed is read after it.
    await waitFor(() => expect(api.asked).toEqual([5]));
  });

  it("offers the messages above again after a read of them failed", async () => {
    api.tails = [fullPage(41), new Error("cannot reach the server"), [said(40)]];
    const { result } = renderHook(() => useThread(7));
    await waitFor(() => expect(result.current.hasOlder).toBe(true));
    act(() => result.current.loadOlder());
    await waitFor(() => expect(result.current.olderError).toBe("cannot reach the server"));
    // What was read stays and is fine, so the chat itself does not report a failure: only the part above it does.
    expect(result.current.error).toBeNull();
    expect(result.current.olderLoading).toBe(false);
    expect(result.current.hasOlder).toBe(true);
    expect(result.current.entries).toHaveLength(PAGE);
    act(() => result.current.loadOlder());
    await waitFor(() => expect(result.current.hasOlder).toBe(false));
    expect(result.current.olderError).toBeNull();
    expect(api.backwards).toEqual([null, 41, 41]);
  });

  it("reads the messages above when the top of the conversation is reached", async () => {
    api.tails = [fullPage(41), [said(40)]];
    const { result } = renderHook(() => useThread(7));
    await waitFor(() => expect(result.current.hasOlder).toBe(true));
    act(() => result.current.loadOlder());
    await waitFor(() => expect(result.current.hasOlder).toBe(false));
    // The page above stops below the oldest message held, and there is nothing above that one.
    expect(api.backwards).toEqual([null, 41]);
    expect(result.current.entries[0]).toMatchObject({ kind: "person", message: "go 40" });
    expect(result.current.olderLoading).toBe(false);
  });

  it("stops offering the messages above once the first of them is held", async () => {
    // The page above is as full as one comes, and it brings the chat's first message: nothing is left above it, so
    // the conversation is whole and there is nothing more to scroll up to.
    api.tails = [fullPage(101), fullPage(1)];
    const { result } = renderHook(() => useThread(7));
    await waitFor(() => expect(result.current.hasOlder).toBe(true));
    act(() => result.current.loadOlder());
    await waitFor(() => expect(result.current.hasOlder).toBe(false));
    expect(result.current.entries[0]).toMatchObject({ kind: "person", message: "go 1" });
  });

  it("asks for the messages above once while one is on its way", async () => {
    let answer: (events: RunEvent[]) => void = () => {};
    const pending = new Promise<RunEvent[]>((resolve) => {
      answer = resolve;
    });
    api.tails = [fullPage(41), pending];
    const { result } = renderHook(() => useThread(7));
    await waitFor(() => expect(result.current.hasOlder).toBe(true));
    act(() => {
      result.current.loadOlder();
      result.current.loadOlder();
    });
    expect(result.current.olderLoading).toBe(true);
    expect(api.backwards).toEqual([null, 41]);
    await act(async () => {
      answer([said(60)]);
      await pending;
    });
    expect(result.current.olderLoading).toBe(false);
    expect(api.backwards).toHaveLength(2);
  });

  it("says nothing is above a chat that is exactly one page of events", async () => {
    // A page as full as one comes whose oldest event is the first of the chat: how full the page was says nothing
    // once the beginning is held, and offering what is above it would offer a page that is not there.
    api.tails = [fullPage(1)];
    const { result } = renderHook(() => useThread(7));
    await waitFor(() => expect(result.current.loaded).toBe(true));
    expect(result.current.entries).toHaveLength(PAGE);
    expect(result.current.hasOlder).toBe(false);
    act(() => result.current.loadOlder());
    expect(api.backwards).toEqual([null]);
  });

  it("says nothing is above a chat that is shorter than the window", async () => {
    api.tails = [[event(1)]];
    const { result } = renderHook(() => useThread(7));
    await waitFor(() => expect(result.current.loaded).toBe(true));
    expect(result.current.hasOlder).toBe(false);
    act(() => result.current.loadOlder());
    expect(api.backwards).toEqual([null]);
  });
});

describe("following a chat", () => {
  it("reads again when the server says it dropped events, and closes the hole", async () => {
    api.tails = [[event(1)]];
    api.pages = [[event(2), event(3)]];
    const { result } = renderHook(() => useThread(7));
    await waitFor(() => expect(result.current.lastSeq).toBe(1));
    // The burst of a fast answer overflowed the queue: the stream will never bring 2 and 3, and only a read will.
    act(() => live.listener?.({ kind: "lagged", data: { missed: 2 } }));
    await waitFor(() => expect(result.current.lastSeq).toBe(3));
    expect(api.asked).toEqual([1]);
    // Everything the stream lost is there again, in the order the `seq`s give it.
    expect(result.current.entries.at(-1)).toMatchObject({
      kind: "text",
      text: "piece 1piece 2piece 3",
    });
  });

  it("ignores an event of another chat and one of no chat at all", async () => {
    api.tails = [[event(1)]];
    api.pages = [[]];
    const { result } = renderHook(() => useThread(7));
    await waitFor(() => expect(result.current.lastSeq).toBe(1));
    act(() => live.listener?.({ kind: "lagged", data: { missed: 1 } }));
    await waitFor(() => expect(api.asked).toHaveLength(1));
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
    api.tails = [[event(1)]];
    const { result, rerender } = renderHook(({ id }: { id: number | null }) => useThread(id), {
      initialProps: { id: 7 as number | null },
    });
    await waitFor(() => expect(result.current.lastSeq).toBe(1));
    rerender({ id: null });
    expect(result.current).toMatchObject({
      entries: [],
      lastSeq: 0,
      loaded: false,
      error: null,
      hasOlder: false,
      olderLoading: false,
      olderError: null,
    });
  });

  it("keeps a chat on screen when reading it again fails while the stream reconnects", async () => {
    api.tails = [[event(1), event(2)]];
    api.pages = [new Error("cannot reach the server")];
    const { result, rerender } = renderHook(() => useThread(7));
    await waitFor(() => expect(result.current.loaded).toBe(true));
    live.connection = "reconnecting";
    rerender();
    await waitFor(() => expect(result.current.error).toBe("cannot reach the server"));
    // The conversation stays; the page shows the reason above it.
    expect(result.current.loaded).toBe(true);
    expect(result.current.entries.at(-1)).toMatchObject({ text: "piece 1piece 2" });
  });

  it("does not let a read that failed late overwrite one that came after it", async () => {
    api.tails = [[event(1)]];
    let fail: (reason: Error) => void = () => {};
    const slow = new Promise<RunEvent[]>((_resolve, reject) => {
      fail = reject;
    });
    api.pages = [slow, [event(2)]];
    const { result, rerender } = renderHook(() => useThread(7));
    await waitFor(() => expect(result.current.loaded).toBe(true));
    // The socket connects twice: the first read is slow and the second is quick.
    live.connection = "reconnecting";
    rerender();
    await waitFor(() => expect(api.asked).toEqual([1]));
    live.connection = "online";
    rerender();
    await waitFor(() => expect(result.current.lastSeq).toBe(2));
    // Now the first one fails. It is the older read, so it says nothing to the page.
    await act(async () => {
      fail(new Error("timed out"));
      await slow.catch(() => {});
    });
    expect(result.current.error).toBeNull();
    expect(result.current.entries.at(-1)).toMatchObject({ text: "piece 1piece 2" });
  });

  it("waits for the first read before reading what the stream missed", async () => {
    // The socket opens again while the end of the chat is still being read: reading from nothing would take the whole
    // log, which is exactly the read the window exists to avoid, so it waits and the first read covers it.
    let answer: (events: RunEvent[]) => void = () => {};
    const slow = new Promise<RunEvent[]>((resolve) => {
      answer = resolve;
    });
    api.tails = [slow];
    const { result, rerender } = renderHook(() => useThread(7));
    await waitFor(() => expect(api.backwards).toEqual([null]));
    live.connection = "reconnecting";
    rerender();
    act(() => live.listener?.({ kind: "lagged", data: { missed: 1 } }));
    expect(api.asked).toEqual([]);
    await act(async () => {
      answer([event(1)]);
      await slow;
    });
    await waitFor(() => expect(result.current.loaded).toBe(true));
    // What was missed while the page was still reading comes in as soon as it has finished.
    await waitFor(() => expect(api.asked).toEqual([1]));
  });
});
