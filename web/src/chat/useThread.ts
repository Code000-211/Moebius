/**
 * Follows one chat: reads its stored events, then takes the `run_event`s of the stream, and gives the page the
 * conversation's entries. Events are pushed into the conversation as they come and the page is told at most once per
 * animation frame, so a fast answer redraws a few dozen times a second, not once per streamed piece. After the socket
 * connected again the events it missed are read.
 */
import { useEffect, useRef, useState } from "react";
import { runEvents } from "../api/calls/chat";
import type { RunEvent } from "../api/types/chat";
import { useConnection, useServerEvents } from "../live/LiveProvider";
import { Conversation, type Entry } from "./conversation";

export interface Thread {
  entries: Entry[];
  /** The highest `seq` the page holds; 0 when none. */
  lastSeq: number;
  /** Whether the stored events were read. */
  loaded: boolean;
  /** Why the events could not be read, if they could not. The entries already read stay in `entries`. */
  error: string | null;
}

const EMPTY: Thread = { entries: [], lastSeq: 0, loaded: false, error: null };

/** The `run_event` of the stream as a stored event, when it is one of chat `id`. */
export function runEventOf(
  id: number,
  event: { kind: string; data: Record<string, unknown> },
): RunEvent | null {
  const { data } = event;
  if (event.kind !== "run_event" || data.run !== id) return null;
  if (typeof data.seq !== "number" || typeof data.kind !== "string") return null;
  const inner =
    typeof data.data === "object" && data.data !== null
      ? (data.data as Record<string, unknown>)
      : {};
  return { seq: data.seq, kind: data.kind, data: inner, at: Math.floor(Date.now() / 1000) };
}

export function useThread(id: number | null): Thread {
  const [thread, setThread] = useState<Thread>(EMPTY);
  const conversation = useRef<{ id: number; value: Conversation; loaded: boolean } | null>(null);
  const frame = useRef<number | null>(null);
  /** The number of the latest read; a result of an older one does not touch the page. */
  const latestRead = useRef(0);
  const connection = useConnection();

  const publish = (error: string | null) => {
    const current = conversation.current;
    if (!current) return;
    setThread({
      entries: current.value.entries(),
      lastSeq: current.value.lastSeq,
      loaded: current.loaded,
      error,
    });
  };

  // Reads what the conversation does not hold yet. Reads are ordinary requests that need no socket, so a chat is read
  // even while the stream is trying to connect again: a laptop waking from sleep used to leave the page on "Reading
  // the chat" until the socket opened, and never mark the chat as read at all.
  // What a read brings is always taken in (reading the same events twice is harmless), but only the latest read says
  // whether the page shows a failure, and a failure never takes a conversation that is on screen away: the page shows
  // it above the entries.
  function refill(current: { id: number; value: Conversation; loaded: boolean }): void {
    const read = ++latestRead.current;
    runEvents(current.id, current.value.lastSeq)
      .then((events) => {
        if (conversation.current !== current) return;
        current.value.backfill(events);
        current.loaded = true;
        if (read === latestRead.current) publish(null);
      })
      .catch((e: unknown) => {
        if (conversation.current !== current || read !== latestRead.current) return;
        publish(e instanceof Error ? e.message : "cannot read the chat");
      });
  }

  // Reads the chat when it is opened, and what it missed whenever the socket is (again) online.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `publish` and `refill` only read refs and set state
  useEffect(() => {
    if (id === null) {
      conversation.current = null;
      setThread(EMPTY);
      return;
    }
    if (conversation.current?.id !== id) {
      conversation.current = { id, value: new Conversation(), loaded: false };
      setThread(EMPTY);
    }
    const current = conversation.current;
    refill(current);
  }, [id, connection]);

  useServerEvents((event) => {
    const current = conversation.current;
    if (!current) return;
    if (event.kind === "lagged") {
      // The server dropped this client's events rather than queue them, so the conversation has a hole in its `seq`s
      // that nothing else fills: the unread marker would never be satisfied and part of the answer would be simply
      // missing. What was missed is still stored, so it is read again.
      refill(current);
      return;
    }
    const stored = runEventOf(current.id, event);
    if (!stored) return;
    current.value.push(stored);
    if (frame.current !== null) return;
    frame.current = requestAnimationFrame(() => {
      frame.current = null;
      publish(null);
    });
  });

  useEffect(
    () => () => {
      // The cancelled frame is no longer coming, so the flag has to go with it or nothing would ever schedule another.
      if (frame.current !== null) cancelAnimationFrame(frame.current);
      frame.current = null;
    },
    [],
  );

  return thread;
}
