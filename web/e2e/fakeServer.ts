/**
 * A small stand-in for the Moebius server, for the end-to-end tests: it keeps to the session contract of the web
 * sign-in (docs/reference/desktop-api.md, "Authentication"), answers the few routes the shell reads, accepts the event
 * WebSocket with the session cookie and serves `dist/` the way the real server serves the embedded app. It listens
 * on port 0. Swap it for a real `moebius serve` with MOEBIUS_E2E_SERVER (see e2e/server.ts).
 */
import { randomBytes } from "node:crypto";
import { existsSync, readFileSync, statSync } from "node:fs";
import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import type { AddressInfo } from "node:net";
import { extname, join, normalize, resolve, sep } from "node:path";
import { type WebSocket, WebSocketServer } from "ws";
import { type BoardFake, createBoardFake } from "./fake/board";
import { createFakeChat, type FakeChat } from "./fake/chat";
import { createPackagesFake, type PackagesFake } from "./fake/packages";
import { createPagesAFake, type PagesAFake } from "./fake/pages-a";
import { createPagesBFake, type PagesBFake } from "./fake/pages-b";
import { createPagesCFake, type PagesCFake } from "./fake/pages-c";
import { createFakeRuns, type FakeRuns } from "./fake/runs";

export const TOKEN = "fake-server-token";
export const AGENT_TOKEN = "fake-run-token";
export const LOGIN_CODE = "fake-login-code";

const DIST = resolve(import.meta.dirname, "../dist");
const POLICIES = JSON.parse(readFileSync(resolve(import.meta.dirname, "../csp.json"), "utf8")) as {
  app: string;
  frame: string;
};
const TYPES: Record<string, string> = {
  ".html": "text/html",
  ".js": "text/javascript",
  ".css": "text/css",
  ".svg": "image/svg+xml",
  ".ttf": "font/ttf",
  ".txt": "text/plain",
};

export interface FakeServer {
  /** `http://127.0.0.1:<port>`. */
  url: string;
  /** What the server printed for a person to open: the login link. */
  loginUrl: string;
  token: string;
  /** Stops listening and cuts every connection: the server is "down". */
  stop(): Promise<void>;
  /** Listens again on the same port after `stop`. */
  restart(): Promise<void>;
  /** Sends an event to every connected event stream. */
  broadcast(event: { kind: string; data: unknown }): void;
  /** The board of the repository `demo` (`fake/board.ts`): move cards, make a route fail, read the requests. */
  board: BoardFake;
  /** The `X-Moebius` header of each non-GET request that carried the cookie, as received. */
  guardLog: Array<string | undefined>;
  /** The chats and the agent that answers in them (e2e/fake/chat.ts). */
  chat: FakeChat;
  /** The runs, their terminals, diffs and the ports (e2e/fake/runs.ts). */
  runs: FakeRuns;
  /** The Inbox, Projects, Fleet, Usage, Get started and Settings routes (e2e/fake/pages-a.ts). */
  pagesA: PagesAFake;
  /** The harness configuration, models and plugins routes (e2e/fake/pages-b.ts). */
  pagesB: PagesBFake;
  /** Rules, automations, Retro, research, the decision model and the column scripts (e2e/fake/pages-c.ts). */
  pagesC: PagesCFake;
  /** The package managers: the Moebius registries and the pi.dev catalog (e2e/fake/packages.ts). */
  packages: PackagesFake;
}

function cookieOf(request: IncomingMessage): string | undefined {
  const header = request.headers.cookie ?? "";
  const match = /(?:^|;\s*)moebius_session=([^;]+)/.exec(header);
  return match?.[1];
}

function readBody(request: IncomingMessage): Promise<string> {
  return new Promise((done) => {
    const chunks: Buffer[] = [];
    request.on("data", (chunk: Buffer) => chunks.push(chunk));
    request.on("end", () => done(Buffer.concat(chunks).toString("utf8")));
  });
}

function text(response: ServerResponse, status: number, body: string): void {
  response.writeHead(status, { "content-type": "text/plain" });
  response.end(body);
}

function json(response: ServerResponse, value: unknown): void {
  response.writeHead(200, { "content-type": "application/json" });
  response.end(JSON.stringify(value));
}

export async function startFakeServer(): Promise<FakeServer> {
  const sessions = new Set<string>();
  const codes = new Set<string>([LOGIN_CODE]);
  const sockets = new Set<WebSocket>();
  const guardLog: Array<string | undefined> = [];
  const events = new WebSocketServer({ noServer: true });
  const broadcast = (event: { kind: string; data: unknown }) => {
    for (const ws of sockets) ws.send(JSON.stringify(event));
  };
  const board = createBoardFake(broadcast);
  const chat = createFakeChat(broadcast);
  const runs = createFakeRuns(broadcast);
  const pagesA = createPagesAFake(broadcast, board);
  const pagesB = createPagesBFake();
  const pagesC = createPagesCFake(broadcast);
  const packages = createPackagesFake(broadcast, pagesA);
  let port = 0;

  const signedIn = (request: IncomingMessage) => {
    const cookie = cookieOf(request);
    return cookie !== undefined && sessions.has(cookie);
  };

  async function api(request: IncomingMessage, response: ServerResponse, path: string) {
    const method = request.method ?? "GET";
    if (method !== "GET" && cookieOf(request) !== undefined) {
      guardLog.push(request.headers["x-moebius"] as string | undefined);
      if (request.headers["x-moebius"] !== "1")
        return text(response, 403, "X-Moebius: 1 is needed");
    }
    if (path === "/v1/session" && method === "POST") {
      const body = JSON.parse((await readBody(request)) || "{}") as {
        token?: string;
        code?: string;
      };
      if (body.token === AGENT_TOKEN) return text(response, 403, "only a person can do this");
      const ok = body.token === TOKEN || (body.code !== undefined && codes.delete(body.code));
      if (!ok) return text(response, 401, "a valid token is needed");
      const session = randomBytes(16).toString("hex");
      sessions.add(session);
      response.writeHead(204, {
        "set-cookie": `moebius_session=${session}; HttpOnly; Path=/; SameSite=Strict`,
      });
      return response.end();
    }
    if (!signedIn(request)) return text(response, 401, "a valid token is needed");
    if (path === "/v1/session" && method === "GET") return json(response, { signed_in: true });
    if (path === "/v1/session" && method === "DELETE") {
      sessions.delete(cookieOf(request) as string);
      response.writeHead(204, { "set-cookie": "moebius_session=; HttpOnly; Path=/; Max-Age=0" });
      return response.end();
    }
    // Answers only once a test of its pages has called use(), so the Settings page's fake keeps /v1/settings otherwise.
    if (await packages.handle(request, response, path)) return;
    if (await pagesB.handle(request, response, path)) return;
    // Rules, automations, Retro, research, the decision model and the column scripts: one rules file is a file of a repository, which pages A would answer.
    if (await pagesC.handle(request, response, path)) return;
    if (await pagesA.handle(request, response, path)) return;
    if (path === "/v1/health") return json(response, { version: "0.0.0-fake" });
    if (path === "/v1/snapshot") return json(response, board.snapshot());
    if (path === "/v1/start") return json(response, { done: true, steps: [], warnings: [] });
    // The runs and the chat read their own request bodies, so they answer before the board reads this one.
    if (await runs.handle(request, response, path)) return;
    if (await chat.handle(request, response, path)) return;
    const body = method === "GET" ? undefined : JSON.parse((await readBody(request)) || "{}");
    const answer = board.handle(method, path, body);
    if (answer) {
      return typeof answer.body === "string"
        ? text(response, answer.status, answer.body)
        : json(response, answer.body);
    }
    return text(response, 404, "no such route");
  }

  function asset(response: ServerResponse, path: string) {
    const relative = normalize(path).replace(/^([/\\])+/, "");
    let file = join(DIST, relative);
    if (!file.startsWith(DIST) || !existsSync(file) || !statSync(file).isFile()) {
      file = join(DIST, "index.html");
    }
    // The policies of the real server (`web/csp.json`, read by `server/api/web.rs`), so the tests run under them.
    const headers: Record<string, string> = {
      "content-type": TYPES[extname(file)] ?? "application/octet-stream",
      "content-security-policy": file.endsWith("plugin-frame.html") ? POLICIES.frame : POLICIES.app,
      "x-content-type-options": "nosniff",
    };
    if (file.includes(`${sep}fonts${sep}`)) headers["access-control-allow-origin"] = "*";
    response.writeHead(200, headers);
    response.end(readFileSync(file));
  }

  const server: Server = createServer((request, response) => {
    const path = new URL(request.url ?? "/", "http://x").pathname;
    if (path.startsWith("/v1/")) {
      void api(request, response, path);
    } else {
      asset(response, path);
    }
  });
  server.on("upgrade", (request, socket, head) => {
    const path = new URL(request.url ?? "/", "http://x").pathname;
    if (signedIn(request) && runs.upgrade(request, socket, head, path)) return;
    if (path !== "/v1/events" || !signedIn(request)) {
      socket.write("HTTP/1.1 401 Unauthorized\r\nConnection: close\r\n\r\n");
      socket.destroy();
      return;
    }
    events.handleUpgrade(request, socket, head, (ws) => {
      sockets.add(ws);
      ws.on("close", () => sockets.delete(ws));
    });
  });

  const listen = () =>
    new Promise<void>((done) => {
      server.listen(port, "127.0.0.1", () => {
        port = (server.address() as AddressInfo).port;
        done();
      });
    });
  await listen();
  const url = `http://127.0.0.1:${port}`;

  return {
    url,
    loginUrl: `${url}/?login=${LOGIN_CODE}`,
    token: TOKEN,
    guardLog,
    board,
    chat,
    runs,
    pagesA,
    pagesB,
    pagesC,
    packages,
    broadcast,
    async stop() {
      for (const ws of sockets) ws.terminate();
      runs.stop();
      server.closeAllConnections();
      await new Promise<void>((done) => server.close(() => done()));
    },
    restart: listen,
  };
}
