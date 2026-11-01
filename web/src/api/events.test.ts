import { describe, expect, it, vi } from "vitest";
import {
  backoff,
  EventStream,
  type EventStreamOptions,
  eventsUrl,
  parseEvent,
  type SocketLike,
} from "./events";

/** A socket the test drives by hand. */
class FakeSocket {
  onopen: SocketLike["onopen"] = null;
  onmessage: SocketLike["onmessage"] = null;
  onclose: SocketLike["onclose"] = null;
  closed = false;
  private readonly asSocket = this as unknown as WebSocket;
  close() {
    this.closed = true;
  }
  open() {
    this.onopen?.call(this.asSocket, new Event("open"));
  }
  say(data: unknown) {
    this.onmessage?.call(this.asSocket, { data } as MessageEvent);
  }
  drop() {
    this.onclose?.call(this.asSocket, new CloseEvent("close"));
  }
}

function setup(extra: Partial<EventStreamOptions> = {}) {
  const sockets: FakeSocket[] = [];
  const waits: number[] = [];
  let next: (() => void) | null = null;
  const events: string[] = [];
  const log: string[] = [];
  const stream = new EventStream({
    url: "ws://x/v1/events",
    onEvent: (event) => events.push(event.kind),
    onConnected: () => log.push("connected"),
    onDisconnected: () => log.push("disconnected"),
    open: () => {
      const socket = new FakeSocket();
      sockets.push(socket);
      return socket as unknown as SocketLike;
    },
    schedule: (run, ms) => {
      waits.push(ms);
      next = run;
      return run;
    },
    cancel: () => {
      next = null;
    },
    ...extra,
  });
  return {
    stream,
    sockets,
    waits,
    events,
    log,
    fire: () => next?.(),
    pending: () => next !== null,
  };
}

describe("backoff", () => {
  it("grows to 30 seconds", () => {
    expect([0, 1, 2, 3, 4, 5, 6, 9].map(backoff)).toEqual([
      1000, 2000, 4000, 8000, 16000, 30000, 30000, 30000,
    ]);
  });
});

describe("eventsUrl", () => {
  it("follows the page's scheme and host", () => {
    expect(eventsUrl({ protocol: "http:", host: "127.0.0.1:7171" })).toBe(
      "ws://127.0.0.1:7171/v1/events",
    );
    expect(eventsUrl({ protocol: "https:", host: "m.example" })).toBe("wss://m.example/v1/events");
  });
});

describe("parseEvent", () => {
  it("reads kind and data", () => {
    expect(parseEvent('{"kind":"card_moved","data":{"card":1}}')).toEqual({
      kind: "card_moved",
      data: { card: 1 },
    });
  });
  it("gives an event without data an empty object", () => {
    expect(parseEvent('{"kind":"x"}')).toEqual({ kind: "x", data: {} });
  });
  it("drops what is not an event", () => {
    expect(parseEvent("not json")).toBeNull();
    expect(parseEvent("[1]")).toBeNull();
    expect(parseEvent('{"data":{}}')).toBeNull();
    expect(parseEvent(new ArrayBuffer(1))).toBeNull();
  });
});

describe("EventStream", () => {
  it("is online once the socket opens and passes events on", () => {
    const t = setup();
    t.stream.start();
    expect(t.stream.getState()).toBe("connecting");
    t.sockets[0]?.open();
    expect(t.stream.getState()).toBe("online");
    t.sockets[0]?.say('{"kind":"card_added","data":{}}');
    t.sockets[0]?.say("junk");
    expect(t.events).toEqual(["card_added"]);
    expect(t.log).toEqual(["connected"]);
  });

  it("connects again after the socket closes, waiting longer each time it fails", () => {
    const t = setup();
    t.stream.start();
    t.sockets[0]?.drop();
    expect(t.stream.getState()).toBe("reconnecting");
    t.fire();
    t.sockets[1]?.drop();
    t.fire();
    t.sockets[2]?.drop();
    expect(t.waits).toEqual([1000, 2000, 4000]);
    expect(t.sockets).toHaveLength(3);
  });

  it("starts the waits again after a successful connection", () => {
    const t = setup();
    t.stream.start();
    t.sockets[0]?.drop();
    t.fire();
    t.sockets[1]?.open();
    t.sockets[1]?.drop();
    expect(t.waits).toEqual([1000, 1000]);
    expect(t.log).toEqual(["disconnected", "connected", "disconnected"]);
  });

  it("reconnects at once when asked, and not while online", () => {
    const t = setup();
    t.stream.start();
    t.sockets[0]?.drop();
    t.stream.reconnectNow();
    expect(t.sockets).toHaveLength(2);
    expect(t.pending()).toBe(false);
    t.sockets[1]?.open();
    t.stream.reconnectNow();
    expect(t.sockets).toHaveLength(2);
  });

  it("treats a socket that cannot be made as a lost connection", () => {
    const open = vi.fn(() => {
      throw new Error("blocked");
    });
    const t = setup({ open });
    t.stream.start();
    expect(t.stream.getState()).toBe("reconnecting");
    expect(t.pending()).toBe(true);
  });

  it("stops for good: no more tries, and the socket is closed", () => {
    const t = setup();
    t.stream.start();
    const socket = t.sockets[0];
    socket?.drop();
    t.stream.stop();
    expect(t.pending()).toBe(false);
    t.stream.start();
    t.stream.stop();
    expect(t.sockets[t.sockets.length - 1]?.closed).toBe(true);
    expect(t.stream.getState()).toBe("connecting");
  });

  it("tells its listeners when the state changes", () => {
    const t = setup();
    const seen: string[] = [];
    t.stream.subscribe(() => seen.push(t.stream.getState()));
    t.stream.start();
    t.sockets[0]?.open();
    t.sockets[0]?.drop();
    expect(seen).toEqual(["online", "reconnecting"]);
  });
});
