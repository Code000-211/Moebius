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
  useSyncExternalStore,
} from "react";
import { type ConnectionState, EventStream, eventsUrl } from "../api/events";
import { keys } from "../api/keys";
import { applyConnected, applyEvent } from "./invalidation";

const LiveContext = createContext<EventStream | null>(null);

export function LiveProvider({ children }: { children: ReactNode }) {
  const queryClient = useQueryClient();
  const stream = useMemo(
    () =>
      new EventStream({
        url: eventsUrl(window.location),
        onEvent: (event) => applyEvent(queryClient, event),
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
  return <LiveContext.Provider value={stream}>{children}</LiveContext.Provider>;
}

function useStream(): EventStream {
  const stream = useContext(LiveContext);
  if (!stream) throw new Error("useConnection needs a LiveProvider");
  return stream;
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
