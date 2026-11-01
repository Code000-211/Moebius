/**
 * The run, terminal, diff and port parts of the fake server. Two runs of the card `#1` of the fake board (`fake/board.ts`)
 * are there from the start: run 1000, which is running with a worktree and events, and run 1001, which has ended. The
 * routes keep to docs/reference/desktop-api.md: `GET /v1/runs/{id}` and `/events`, `POST /v1/runs/{id}/steer` (a
 * `steered` event, 422 for a blank message or a run that is not running), `GET /v1/runs/{id}/diff` and
 * `GET /v1/changes/{id}/diff`, `GET /v1/ports`. The terminals speak the server's protocol exactly: binary messages
 * carry bytes both ways and a text message `{"type":"resize","cols","rows"}` sets the size; the fake shell prints a
 * prompt, switches on bracketed paste as a real shell does, and echoes what it is sent. Anything else a client sends is
 * kept in `invalid` and closes the socket.
 */
import type { IncomingMessage, ServerResponse } from "node:http";
import type { Duplex } from "node:stream";
import { type RawData, type WebSocket, WebSocketServer } from "ws";

const NOW = Math.floor(Date.now() / 1000);

/** The escape sequences that switch bracketed paste on, and that frame a paste. */
export const BRACKETED_PASTE_ON = "\x1b[?2004h";
export const PASTE_START = "\x1b[200~";
export const PASTE_END = "\x1b[201~";

export interface FakeTerminal {
  /** The path it was opened on, such as `/v1/terminal`. */
  path: string;
  /** Each binary message the client sent, in order. */
  readonly input: Buffer[];
  /** Each valid resize the client sent, in order. */
  readonly resizes: Array<{ cols: number; rows: number }>;
  /** Text messages that are no resize, and anything else the protocol does not allow. */
  readonly invalid: string[];
  /** Everything the client sent, as text. */
  typed(): string;
  /** Prints `text` in the terminal. */
  print(text: string): void;
  closed: boolean;
}

export interface FakeRuns {
  /** Answers a route of the runs and returns true, or returns false for any other path. */
  handle(request: IncomingMessage, response: ServerResponse, path: string): Promise<boolean>;
  /** Accepts the upgrade of a terminal path and returns true, or returns false for any other path. */
  upgrade(request: IncomingMessage, socket: Duplex, head: Buffer, path: string): boolean;
  /** The terminals opened, oldest first. */
  readonly terminals: FakeTerminal[];
  /** The bodies of `POST /v1/runs/{id}/steer`, as received. */
  readonly steered: Array<{ run: number; body: Record<string, unknown> }>;
  /** The ports `GET /v1/ports` lists. */
  ports: Array<{ port: number; pid: number; process: string; kind: string; run: number | null }>;
  /** Answers every steer with 422 and this message while it is set. */
  refuseSteer: string | null;
  /** Refuses the next terminals with this status (the browser only sees that the socket did not open). */
  refuseTerminals: number | null;
  /** Stores an event of the run and sends it on the event socket; returns its `seq`. */
  emit(run: number, kind: string, data?: Record<string, unknown>): number;
  /** Cuts the terminals, as the server stopping does. */
  stop(): void;
}

interface FakeRun {
  id: number;
  card_id: number | null;
  agent: string;
  harness: string;
  purpose: string;
  status: string;
  worktree: string | null;
  started_at: number;
  ended_at: number | null;
  summary: string | null;
  repository_id: number | null;
  harness_repository: boolean;
  origin: string;
  runner: string | null;
}

const RUN_DIFF = `diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,4 +1,5 @@
 pub fn greet() {
-    println!("hello");
+    println!("hello, world");
+    println!("welcome");
 }
 
diff --git a/notes.txt b/notes.txt
new file mode 100644
index 0000000..3333333
--- /dev/null
+++ b/notes.txt
@@ -0,0 +1 @@
+a note for the reviewer
`;

const CHANGE_DIFF = `diff --git a/Makefile b/Makefile
index 4444444..5555555 100644
--- a/Makefile
+++ b/Makefile
@@ -1,3 +1,3 @@
 all:
-	cc -O0 main.c
+	cc -O2 main.c
 clean:
`;

function readBody(request: IncomingMessage): Promise<string> {
  return new Promise((done) => {
    const chunks: Buffer[] = [];
    request.on("data", (chunk: Buffer) => chunks.push(chunk));
    request.on("end", () => done(Buffer.concat(chunks).toString("utf8")));
  });
}

function reply(response: ServerResponse, status: number, body: unknown): true {
  const text = typeof body === "string";
  response.writeHead(status, { "content-type": text ? "text/plain" : "application/json" });
  response.end(text ? body : JSON.stringify(body));
  return true;
}

/** A terminal path of the server and whether it is one. */
const TERMINAL_PATH =
  /^\/v1\/(?:runs\/\d+\/terminal|chats\/\d+\/tui|terminal|repositories\/\d+\/terminal)$/;

export function createFakeRuns(
  broadcast: (event: { kind: string; data: unknown }) => void,
): FakeRuns {
  const runs: FakeRun[] = [
    {
      id: 1000,
      card_id: 1,
      agent: "Coder",
      harness: "pi-bolt",
      purpose: "code",
      status: "running",
      worktree: "/srv/.moebius/runs/1000/worktree",
      started_at: NOW - 600,
      ended_at: null,
      summary: null,
      repository_id: null,
      harness_repository: false,
      origin: "chat",
      runner: null,
    },
    {
      id: 1001,
      card_id: 5,
      agent: "Coder",
      harness: "pi-bolt",
      purpose: "code",
      status: "succeeded",
      worktree: null,
      started_at: NOW - 5000,
      ended_at: NOW - 4000,
      summary: "Done.",
      repository_id: null,
      harness_repository: false,
      origin: "chat",
      runner: null,
    },
  ];
  const events = new Map<number, Array<{ seq: number; kind: string; data: object; at: number }>>();
  const terminals: FakeTerminal[] = [];
  const sockets = new Set<WebSocket>();
  const steered: FakeRuns["steered"] = [];
  const shell = new WebSocketServer({ noServer: true });

  const fake: FakeRuns = {
    terminals,
    steered,
    refuseTerminals: null,
    refuseSteer: null,
    ports: [
      { port: 3000, pid: 4242, process: "node", kind: "run", run: 1000 },
      { port: 8080, pid: 4300, process: "python3", kind: "terminal", run: null },
    ],

    emit(run, kind, data = {}) {
      const list = events.get(run) ?? [];
      events.set(run, list);
      const seq = (list.at(-1)?.seq ?? 0) + 1;
      list.push({ seq, kind, data, at: Math.floor(Date.now() / 1000) });
      broadcast({ kind: "run_event", data: { run, seq, kind, data } });
      return seq;
    },

    async handle(request, response, path) {
      const method = request.method ?? "GET";
      const url = new URL(request.url ?? "/", "http://x");
      if (path === "/v1/ports" && method === "GET") return reply(response, 200, fake.ports);
      if (path === "/v1/changes/1/diff" && method === "GET") {
        return reply(response, 200, { diff: CHANGE_DIFF, truncated: false });
      }
      const one = /^\/v1\/runs\/(\d+)(\/(?:events|steer|diff))?$/.exec(path);
      if (!one) return false;
      const id = Number(one[1]);
      const run = runs.find((each) => each.id === id);
      if (!run) return false;
      if (method === "GET" && one[2] === undefined) return reply(response, 200, run);
      if (method === "GET" && one[2] === "/diff") {
        return reply(response, 200, {
          diff: run.status === "running" ? RUN_DIFF : "",
          truncated: false,
        });
      }
      if (method === "GET" && one[2] === "/events") {
        const after = Number(url.searchParams.get("after") ?? 0);
        const limit = Number(url.searchParams.get("limit") ?? 500);
        const page = (events.get(id) ?? []).filter((event) => event.seq > after).slice(0, limit);
        return reply(response, 200, page);
      }
      if (method === "POST" && one[2] === "/steer") {
        const body = JSON.parse((await readBody(request)) || "{}") as Record<string, unknown>;
        steered.push({ run: id, body });
        const message = typeof body.message === "string" ? body.message : "";
        if (message.trim() === "") return reply(response, 422, "a steer needs a message");
        if (run.status !== "running" || fake.refuseSteer !== null) {
          return reply(response, 422, fake.refuseSteer ?? `run ${id} is not running`);
        }
        fake.emit(id, "steered", { message });
        return reply(response, 200, run);
      }
      return false;
    },

    upgrade(request, socket, head, path) {
      if (!TERMINAL_PATH.test(path)) return false;
      if (fake.refuseTerminals !== null) {
        socket.write(`HTTP/1.1 ${fake.refuseTerminals} Refused\r\nConnection: close\r\n\r\n`);
        socket.destroy();
        return true;
      }
      shell.handleUpgrade(request, socket, head, (ws) => {
        sockets.add(ws);
        const input: Buffer[] = [];
        const terminal: FakeTerminal = {
          path,
          input,
          resizes: [],
          invalid: [],
          closed: false,
          typed: () => Buffer.concat(input).toString("utf8"),
          print: (text) => ws.send(Buffer.from(text, "utf8"), { binary: true }),
        };
        terminals.push(terminal);
        ws.on("close", () => {
          terminal.closed = true;
          sockets.delete(ws);
        });
        ws.on("message", (data: RawData, isBinary: boolean) => {
          if (isBinary) {
            const bytes = Buffer.from(data as Buffer);
            input.push(bytes);
            const echo = bytes
              .toString("utf8")
              .replaceAll(PASTE_START, "")
              .replaceAll(PASTE_END, "")
              .replaceAll("\r", "\r\n$ ");
            terminal.print(echo);
            return;
          }
          const text = (data as Buffer).toString("utf8");
          try {
            const message = JSON.parse(text) as { type?: unknown; cols?: unknown; rows?: unknown };
            const { cols, rows } = message;
            if (
              message.type === "resize" &&
              typeof cols === "number" &&
              typeof rows === "number" &&
              Number.isInteger(cols) &&
              Number.isInteger(rows) &&
              cols >= 1 &&
              rows >= 1
            ) {
              terminal.resizes.push({ cols, rows });
              return;
            }
          } catch {
            // Falls through: not JSON.
          }
          terminal.invalid.push(text);
          ws.close(1002, "not a resize");
        });
        terminal.print(`${BRACKETED_PASTE_ON}$ `);
      });
      return true;
    },

    stop() {
      for (const ws of sockets) ws.terminate();
    },
  };

  fake.emit(1000, "agent_start", { type: "agent_start" });
  fake.emit(1000, "message_update", {
    type: "message_update",
    assistantMessageEvent: {
      type: "text_delta",
      contentIndex: 0,
      delta: "I am reading the build files.",
    },
  });
  return fake;
}
