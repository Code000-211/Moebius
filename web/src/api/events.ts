/**
 * The event stream of the server (`GET /v1/events`): one WebSocket that connects again by itself with a growing wait
 * and tells who listens about the connection and every event. Events are not replayed, so a listener reads what it
 * shows again whenever `onConnected` is called. The socket is authenticated by the session cookie.
 */
import type { ServerEvent } from "./types/events";

export type ConnectionState = "connecting" | "online" | "reconnecting";

/** The part of a WebSocket the stream uses, so a test can stand a fake in. */
export type SocketLike = Pick<WebSocket, "onopen" | "onmessage" | "onclose" | "close">;

export interface EventStreamOptions {
  url: string;
  onEvent: (event: ServerEvent) => void;
  /** The socket is open; the events of the time before it are lost, so read again what is shown. */
  onConnected?: () => void;
  /** The socket closed or could not be opened; the stream tries again. */
  onDisconnected?: () => void;
  /** Opens a socket; tests replace it. */
  open?: (url: string) => SocketLike;
  /** Schedules the next try; tests replace it. */
  schedule?: (run: () => void, ms: number) => unknown;
  cancel?: (handle: unknown) => void;
}

/** How long to wait before trying again after the `failures`-th failure in a row, in milliseconds: 1, 2, 4 ... 30 s. */
export function backoff(failures: number): number {
  return Math.min(2 ** Math.min(failures, 5), 30) * 1000;
}

/** The WebSocket address of the events of the server that served the page. */
export function eventsUrl(location: Pick<Location, "protocol" | "host">): string {
  const scheme = location.protocol === "https:" ? "wss:" : "ws:";
  return `${scheme}//${location.host}/v1/events`;
}

/** Reads one message of the socket; anything that is not an event object is dropped. */
export function parseEvent(data: unknown): ServerEvent | null {
  if (typeof data !== "string") return null;
  try {
    const value: unknown = JSON.parse(data);
    if (typeof value !== "object" || value === null) return null;
    const { kind, data: payload } = value as { kind?: unknown; data?: unknown };
    if (typeof kind !== "string") return null;
    const object =
      typeof payload === "object" && payload !== null ? (payload as Record<string, unknown>) : {};
    return { kind, data: object };
  } catch {
    return null;
  }
}

export class EventStream {
  private state: ConnectionState = "connecting";
  private readonly listeners = new Set<() => void>();
  private socket: SocketLike | null = null;
  private timer: unknown = null;
  private failures = 0;
  private stopped = true;

  constructor(private readonly options: EventStreamOptions) {}

  /** The state of the connection, for `useSyncExternalStore`. */
  getState = (): ConnectionState => this.state;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  start(): void {
    if (!this.stopped) return;
    this.stopped = false;
    this.connect();
  }

  /** Closes the socket for good; `start` opens it again. */
  stop(): void {
    this.stopped = true;
    this.clearTimer();
    const socket = this.socket;
    this.socket = null;
    if (socket) {
      socket.onopen = socket.onmessage = socket.onclose = null;
      socket.close();
    }
    this.setState("connecting");
  }

  /** Does not wait for the next scheduled try: connects now, unless the socket is open. */
  reconnectNow(): void {
    if (this.stopped || this.state === "online") return;
    this.clearTimer();
    this.connect();
  }

  private connect(): void {
    this.socket?.close();
    const open = this.options.open ?? ((url) => new WebSocket(url));
    let socket: SocketLike;
    try {
      socket = open(this.options.url);
    } catch {
      this.lost();
      return;
    }
    this.socket = socket;
    socket.onopen = () => {
      this.failures = 0;
      this.setState("online");
      this.options.onConnected?.();
    };
    socket.onmessage = (message) => {
      const event = parseEvent(message.data);
      if (event) this.options.onEvent(event);
    };
    socket.onclose = () => {
      if (this.socket === socket) this.lost();
    };
  }

  private lost(): void {
    this.socket = null;
    this.setState("reconnecting");
    this.options.onDisconnected?.();
    if (this.stopped) return;
    const schedule = this.options.schedule ?? ((run, ms) => setTimeout(run, ms));
    const wait = backoff(this.failures);
    this.failures += 1;
    this.timer = schedule(() => {
      this.timer = null;
      if (!this.stopped) this.connect();
    }, wait);
  }

  private clearTimer(): void {
    if (this.timer === null) return;
    (this.options.cancel ?? ((handle) => clearTimeout(handle as number)))(this.timer);
    this.timer = null;
  }

  private setState(state: ConnectionState): void {
    if (state === this.state) return;
    this.state = state;
    for (const listener of this.listeners) listener();
  }
}
