/**
 * A small stand-in for the Moebius server, for the end-to-end tests: it keeps to the session contract of the web
 * sign-in (docs/reference/desktop-api.md, "Authentication"), with the server's token for the first owner and the
 * access tokens of the people an owner adds with `POST /v1/people`, answers the few routes the shell reads, accepts the event
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
import { type CyclesFake, createCyclesFake } from "./fake/cycles";
import { createKindsFake, type KindsFake } from "./fake/kinds";
import { createMilestonesFake, type MilestonesFake } from "./fake/milestones";
import { createPackagesFake, type PackagesFake } from "./fake/packages";
import { createPagesAFake, type PagesAFake } from "./fake/pages-a";
import { createPagesBFake, type PagesBFake } from "./fake/pages-b";
import { createPagesCFake, type PagesCFake } from "./fake/pages-c";
import { createFakeRuns, type FakeRuns } from "./fake/runs";
import { createTemplatesFake, type TemplatesFake } from "./fake/templates";
import { createViewsFake, type ViewsFake } from "./fake/views";
import { createWikiFake, type WikiFake } from "./fake/wiki";
import { createWorkflowsFake, type WorkflowsFake } from "./fake/workflows";

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
  /** How many event streams are open now: counted up when one connects and down when it closes. */
  openStreams(): number;
  /** How many runs the open event streams have asked for the events of, each counted once. */
  watchedRuns(): number;
  /** Sends an event to every connected event stream. */
  broadcast(event: { kind: string; data: unknown }): void;
  /** Tells the person with `handle` something, as the server does for a mention: the notice and its event. */
  notify(
    handle: string,
    notice: {
      kind: "assigned" | "mentioned" | "changed";
      by?: string;
      card_id?: number;
      title: string;
      body: string;
    },
  ): void;
  /** The board of the repository `demo` (`fake/board.ts`): move cards, make a route fail, read the requests. */
  board: BoardFake;
  /** The `X-Moebius` header of each non-GET request that carried the cookie, as received. */
  guardLog: Array<string | undefined>;
  /** The chats and the agent that answers in them (e2e/fake/chat.ts). */
  chat: FakeChat;
  /** The runs, their terminals, diffs and the ports (e2e/fake/runs.ts). */
  runs: FakeRuns;
  /** The cycles of the board's project and a card's cycle (e2e/fake/cycles.ts). */
  cycles: CyclesFake;
  /** The milestones and a card's milestones (e2e/fake/milestones.ts). */
  milestones: MilestonesFake;
  /** The saved views (e2e/fake/views.ts). */
  views: ViewsFake;
  /** The Inbox, Projects, Fleet, Usage, Get started and Settings routes (e2e/fake/pages-a.ts). */
  pagesA: PagesAFake;
  /** The harness configuration, models and plugins routes (e2e/fake/pages-b.ts). */
  pagesB: PagesBFake;
  /** Rules, automations, Retro, research, the decision model and the column scripts (e2e/fake/pages-c.ts). */
  pagesC: PagesCFake;
  /** The package managers: the Moebius registries and the pi.dev catalog (e2e/fake/packages.ts). */
  packages: PackagesFake;
  /** The workflows and their runs (e2e/fake/workflows.ts). */
  workflows: WorkflowsFake;
  /** The wiki's pages and their versions (e2e/fake/wiki.ts). */
  wiki: WikiFake;
  /** The kinds of issues and the issues across projects (e2e/fake/kinds.ts). */
  kinds: KindsFake;
  /** The issue and page templates, the secrets and the Jira import (e2e/fake/templates.ts). */
  templates: TemplatesFake;
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
  /** Each session's person, by id. */
  /** What each person chose for themselves (`/v1/preferences`), by person id. */
  const preferences = new Map<
    number,
    { show_thinking_expanded: boolean; wrap_long_lines: boolean }
  >();
  const sessions = new Map<string, number>();
  const people = [
    { id: 1, handle: "owner", name: "Owner", role: "owner", disabled: false, created_at: 0 },
  ];
  /** Each access token's record, by the token itself. */
  const accessTokens = new Map<
    string,
    {
      id: number;
      person_id: number;
      name: string;
      created_at: number;
      last_used_at: number | null;
      revoked_at: number | null;
    }
  >();
  /** The notices of each person, as the Mentions tab reads them, and who follows which card (person ids). */
  const notices: Array<{ person: number; notice: Record<string, unknown> }> = [];
  const followers = new Map<number, Set<number>>();
  /** The person a token stands for: the first owner for the server's token; none for a revoked token or a disabled person. */
  const personOf = (token: string | undefined) => {
    if (token === TOKEN) return 1;
    const record = token === undefined ? undefined : accessTokens.get(token);
    if (!record || record.revoked_at !== null) return undefined;
    if (people.find((p) => p.id === record.person_id)?.disabled) return undefined;
    record.last_used_at = Math.floor(Date.now() / 1000);
    return record.person_id;
  };
  const issue = (person: number, name: string) => {
    const token = `moebius-access-${randomBytes(32).toString("hex")}`;
    const record = {
      id: accessTokens.size + 1,
      person_id: person,
      name,
      created_at: 0,
      last_used_at: null,
      revoked_at: null,
    };
    accessTokens.set(token, record);
    return { ...record, token };
  };
  const codes = new Set<string>([LOGIN_CODE]);
  const sockets = new Set<WebSocket>();
  /** The runs each open event stream has asked for. */
  const watching = new Map<WebSocket, Set<number>>();
  const guardLog: Array<string | undefined> = [];
  const events = new WebSocketServer({ noServer: true });
  const broadcast = (event: { kind: string; data: unknown }) => {
    for (const ws of sockets) ws.send(JSON.stringify(event));
  };
  const board = createBoardFake(broadcast);
  const chat = createFakeChat(broadcast);
  const runs = createFakeRuns(broadcast);
  const pagesA = createPagesAFake(broadcast, board);
  const cycles = createCyclesFake(broadcast);
  const milestones = createMilestonesFake(broadcast);
  const views = createViewsFake();
  const pagesB = createPagesBFake();
  const pagesC = createPagesCFake(broadcast);
  const workflows = createWorkflowsFake(() => pagesC.automationList());
  const kinds = createKindsFake(broadcast);
  const templates = createTemplatesFake(kinds);
  board.issueTemplate = templates.issueTemplate;
  const wiki = createWikiFake(broadcast, pagesA, templates);
  const packages = createPackagesFake(broadcast, pagesA);
  let port = 0;

  const signedIn = (request: IncomingMessage) => {
    const cookie = cookieOf(request);
    if (cookie === undefined || !sessions.has(cookie)) return false;
    // A person who was disabled since is signed out at once.
    return !people.find((p) => p.id === sessions.get(cookie))?.disabled;
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
        stay?: boolean;
      };
      if (body.token === AGENT_TOKEN) return text(response, 403, "only a person can do this");
      const person =
        personOf(body.token) ??
        (body.code !== undefined && codes.delete(body.code) ? 1 : undefined);
      if (person === undefined) return text(response, 401, "a valid token is needed");
      const session = randomBytes(16).toString("hex");
      sessions.set(session, person);
      response.writeHead(204, {
        "set-cookie": `moebius_session=${session}; HttpOnly; Path=/; SameSite=Strict${
          body.stay === false ? "" : "; Max-Age=2592000"
        }`,
      });
      return response.end();
    }
    // An owner adds a person with the server's token, as a script would, and gets their first access token.
    const bearer = /^Bearer (.+)$/.exec(request.headers.authorization ?? "")?.[1];
    if (path === "/v1/people" && method === "POST" && personOf(bearer) === 1) {
      const body = JSON.parse(await readBody(request)) as {
        handle: string;
        name: string;
        role: string;
      };
      const person = { id: people.length + 1, ...body, disabled: false, created_at: 0 };
      people.push(person);
      return json(response, { person, token: issue(person.id, "first token") });
    }
    if (!signedIn(request)) return text(response, 401, "a valid token is needed");
    const asker = sessions.get(cookieOf(request) as string) as number;
    const askerRole = people.find((p) => p.id === asker)?.role;
    const mine = (id: number) => [...accessTokens.entries()].filter(([, t]) => t.person_id === id);
    if (path === "/v1/preferences") {
      if (method === "PUT") {
        const body = JSON.parse(await readBody(request)) as {
          show_thinking_expanded?: boolean;
          wrap_long_lines?: boolean;
        };
        preferences.set(asker, {
          show_thinking_expanded: body.show_thinking_expanded ?? false,
          wrap_long_lines: body.wrap_long_lines ?? false,
        });
      }
      return json(
        response,
        preferences.get(asker) ?? { show_thinking_expanded: false, wrap_long_lines: false },
      );
    }
    if (path === "/v1/people" && method === "GET") return json(response, people);
    if (path === "/v1/people" && method === "POST") {
      if (askerRole !== "owner") return text(response, 403, "only an owner can do this");
      const body = JSON.parse(await readBody(request)) as {
        handle: string;
        name: string;
        role: string;
      };
      if (people.some((p) => p.handle === body.handle))
        return text(response, 409, `@${body.handle} is taken`);
      const person = { id: people.length + 1, ...body, disabled: false, created_at: 0 };
      people.push(person);
      return json(response, { person, token: issue(person.id, "first token") });
    }
    const personRoute = /^\/v1\/people\/(\d+)(\/tokens)?$/.exec(path);
    if (personRoute) {
      const id = Number(personRoute[1]);
      const person = people.find((p) => p.id === id);
      if (askerRole !== "owner" && !(personRoute[2] && id === asker))
        return text(response, 403, "only an owner can do this");
      if (!person) return text(response, 404, `person ${id}`);
      if (personRoute[2] && method === "GET")
        return json(
          response,
          mine(id).map(([, t]) => t),
        );
      if (!personRoute[2] && method === "PATCH") {
        const body = JSON.parse(await readBody(request)) as { role?: string; disabled?: boolean };
        if (body.role !== undefined) person.role = body.role;
        if (body.disabled !== undefined) person.disabled = body.disabled;
        return json(response, person);
      }
    }
    if (path === "/v1/tokens" && method === "GET")
      return json(
        response,
        mine(asker).map(([, t]) => t),
      );
    if (path === "/v1/tokens" && method === "POST") {
      const body = JSON.parse(await readBody(request)) as { name: string };
      return json(response, issue(asker, body.name));
    }
    const tokenRoute = /^\/v1\/tokens\/(\d+)$/.exec(path);
    if (tokenRoute && method === "DELETE") {
      const found = [...accessTokens.values()].find((t) => t.id === Number(tokenRoute[1]));
      if (!found || (found.person_id !== asker && askerRole !== "owner"))
        return text(response, 404, `token ${tokenRoute[1]}`);
      found.revoked_at = 1;
      return json(response, found);
    }
    if (path === "/v1/notices" && method === "GET")
      return json(
        response,
        notices
          .filter((n) => n.person === asker)
          .map((n) => n.notice)
          .reverse(),
      );
    const read = /^\/v1\/notices\/(\d+)\/read$/.exec(path);
    if (read && method === "POST") {
      const found = notices.find((n) => n.person === asker && n.notice.id === Number(read[1]));
      if (!found) return text(response, 404, `notice ${read[1]}`);
      found.notice.read = true;
      broadcast({
        kind: "notice_read",
        data: { person: people.find((p) => p.id === asker)?.handle, notice: found.notice },
      });
      return json(response, found.notice);
    }
    const follows = /^\/v1\/cards\/(\d+)\/(followers|follow)$/.exec(path);
    if (follows) {
      const card = Number(follows[1]);
      const set = followers.get(card) ?? new Set<number>();
      if (follows[2] === "follow" && method === "PUT") {
        const body = JSON.parse((await readBody(request)) || "{}") as { following?: boolean };
        if (body.following) set.add(asker);
        else set.delete(asker);
        followers.set(card, set);
      }
      return json(
        response,
        people.filter((p) => set.has(p.id)).map((p) => p.handle),
      );
    }
    if (path === "/v1/session" && method === "GET") {
      const id = sessions.get(cookieOf(request) as string);
      return json(response, { signed_in: true, person: people.find((p) => p.id === id) ?? null });
    }
    if (path === "/v1/session" && method === "DELETE") {
      sessions.delete(cookieOf(request) as string);
      response.writeHead(204, { "set-cookie": "moebius_session=; HttpOnly; Path=/; Max-Age=0" });
      return response.end();
    }
    const person = people.find((p) => p.id === asker);
    if (person?.role === "viewer" && request.method !== "GET" && path.startsWith("/v1/wiki/"))
      return text(response, 403, "a viewer reads the wiki");
    if (await wiki.handle(request, response, path, person ?? { name: "Owner", handle: "owner" }))
      return;
    // Before the pages that answer `PUT /v1/secrets/{name}` too: a Jira token is read back from here.
    if (await templates.handle(request, response, path)) return;
    // Answers only once a test of its pages has called use(), so the Settings page's fake keeps /v1/settings otherwise.
    if (await cycles.handle(request, response, path)) return;
    if (await milestones.handle(request, response, path)) return;
    if (await views.handle(request, response, path)) return;
    if (await packages.handle(request, response, path)) return;
    if (await pagesB.handle(request, response, path)) return;
    // Rules, automations, Retro, research, the decision model and the column scripts: one rules file is a file of a repository, which pages A would answer.
    if (await pagesC.handle(request, response, path)) return;
    if (await kinds.handle(request, response, path)) return;
    if (await pagesA.handle(request, response, path)) return;
    // The workflows' own routes read their path before the board does; none of them changes anything.
    if (await workflows.handle(request, response, path)) return;
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
      const asked = new Set<number>();
      watching.set(ws, asked);
      ws.on("message", (data) => {
        try {
          const request = JSON.parse(String(data)) as {
            subscribe?: number[];
            unsubscribe?: number[];
          };
          for (const id of request.subscribe ?? []) asked.add(id);
          for (const id of request.unsubscribe ?? []) asked.delete(id);
        } catch {
          // Not a request of the stream; nothing to count.
        }
      });
      ws.on("close", () => {
        sockets.delete(ws);
        watching.delete(ws);
      });
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
    openStreams: () => sockets.size,
    watchedRuns: () => new Set([...watching.values()].flatMap((asked) => [...asked])).size,
    board,
    chat,
    runs,
    cycles,
    milestones,
    views,
    pagesA,
    pagesB,
    pagesC,
    packages,
    workflows,
    wiki,
    kinds,
    templates,
    broadcast,
    notify(handle, notice) {
      const person = people.find((p) => p.handle === handle);
      if (!person) throw new Error(`no person @${handle}`);
      const made = {
        id: notices.length + 1,
        by: null,
        card_id: null,
        inbox_item_id: null,
        at: Math.floor(Date.now() / 1000),
        read: false,
        ...notice,
      };
      notices.push({ person: person.id, notice: made });
      broadcast({ kind: "notice_added", data: { person: handle, notice: made } });
    },
    async stop() {
      for (const ws of sockets) ws.terminate();
      runs.stop();
      server.closeAllConnections();
      await new Promise<void>((done) => server.close(() => done()));
    },
    restart: listen,
  };
}
