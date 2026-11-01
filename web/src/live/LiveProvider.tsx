/**
 * Opens the event stream while a person is signed in, feeds it to the query cache, and gives the pages the state of
 * the connection. When the socket cannot be opened or closes, the session is asked again: a server that answers 401
 * sends the person back to the sign-in page, one that does not answer shows the banner.
 */
import { useQueryClient } from "@tanstack/react-query";
import {
  createContext,
  type ReactNode,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useSyncExternalStore,
} from "react";
import { type ConnectionState, EventStream, eventsUrl } from "../api/events";
import { keys } from "../api/keys";
import type { ServerEvent } from "../api/types/events";
import { applyConnected, applyEvent } from "./invalidation";

type Listener = (event: ServerEvent) => void;

interface Live {
  stream: EventStream;
  /** Listens to every event, after the cache has taken it; returns the way to stop. */
  listen: (listener: Listener) => () => void;
}

const LiveContext = createContext<Live | null>(null);

export function LiveProvider({ children }: { children: ReactNode }) {
  const queryClient = useQueryClient();
  const listeners = useRef(new Set<Listener>());
  const stream = useMemo(
    () =>
      new EventStream({
        url: eventsUrl(window.location),
        onEvent: (event) => {
          applyEvent(queryClient, event);
          for (const listener of listeners.current) listener(event);
        },
        onConnected: () => applyConnected(queryClient),
        onDisconnected: () => {
          void queryClient.invalidateQueries({ queryKey: keys.session });
        },
      }),
    [queryClient],
  );
  useEffect(() => {
    stream.start();
    return () => stream.stop();
  }, [stream]);
  const live = useMemo<Live>(
    () => ({
      stream,
      listen: (listener) => {
        listeners.current.add(listener);
        return () => listeners.current.delete(listener);
      },
    }),
    [stream],
  );
  return <LiveContext.Provider value={live}>{children}</LiveContext.Provider>;
}

function useLive(): Live {
  const live = useContext(LiveContext);
  if (!live) throw new Error("the live hooks need a LiveProvider");
  return live;
}

function useStream(): EventStream {
  return useLive().stream;
}

/** Calls `listener` with every event of the server while the component is mounted. */
export function useServerEvents(listener: Listener): void {
  const { listen } = useLive();
  const latest = useRef(listener);
  latest.current = listener;
  useEffect(() => listen((event) => latest.current(event)), [listen]);
}

/** The state of the event stream. */
export function useConnection(): ConnectionState {
  const stream = useStream();
  return useSyncExternalStore(stream.subscribe, stream.getState);
}

/** Tries to connect at once instead of at the next scheduled try. */
export function useReconnect(): () => void {
  const stream = useStream();
  return () => stream.reconnectNow();
}
