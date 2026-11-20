/**
 * The routes of kinds of issues in the fake server, with the JSON the real one sends (docs/reference/desktop-api.md,
 * "Kinds of issues"): the kinds with their fields and the projects that picked them, and `GET /v1/issues`, which
 * filters and sorts a few seeded cards of two projects (id 1, the demo repository's, and id 100, the first project a
 * test makes) by the fields of the kind `Bug`. A kind's fields are checked as the real server checks them, enough for the
 * pages: a name twice is refused with the server's sentence.
 */
import type { IncomingMessage, ServerResponse } from "node:http";

type Send = (event: { kind: string; data: unknown }) => void;
type Obj = Record<string, unknown>;

interface Field {
  id: number;
  name: string;
  type: string;
  options: string[];
}

interface Kind {
  id: number;
  name: string;
  fields: Field[];
  projects: number[];
}

export interface KindsFake {
  /** Answers one of these routes and returns true, or returns false for any other path. */
  handle(request: IncomingMessage, response: ServerResponse, path: string): Promise<boolean>;
  /** The kinds as they are now. */
  list(): Array<{ name: string; fields: Array<{ name: string }> }>;
  /** Every request made to these routes, as method, path with query, and body. */
  readonly calls: Array<{ method: string; path: string; body: unknown }>;
}

/** The cards `GET /v1/issues` answers from: two projects, each with issues of the kind `Bug`. */
const CARDS = [
  { id: 1, project: 1, number: 1, title: "Login is broken", severity: "high", estimate: 5 },
  { id: 2, project: 1, number: 2, title: "A typo on the home page", severity: "low", estimate: 1 },
  { id: 3, project: 100, number: 1, title: "Checkout fails", severity: "high", estimate: 2 },
].map((c) => ({
  id: c.id,
  repository_id: c.project,
  project_id: c.project,
  number: c.number,
  title: c.title,
  body: "",
  author: "person",
  column: "Backlog",
  created_at: 0,
  note: null,
  retried_after: null,
  confirmed: false,
  held: null,
  parent: null,
  closed: false,
  labels: [],
  claimed_by: null,
  // The first owner has the first and the third; someone else the second (the My issues page tells them apart).
  assignees: [c.id === 2 ? "tom" : "owner"],
  kind: "Bug",
  fields: { Severity: c.severity, Estimate: c.estimate },
}));

function json(response: ServerResponse, status: number, body: unknown): true {
  if (typeof body === "string") {
    response.writeHead(status, { "content-type": "text/plain" });
    response.end(body);
  } else {
    response.writeHead(status, { "content-type": "application/json" });
    response.end(JSON.stringify(body));
  }
  return true;
}

function readBody(request: IncomingMessage): Promise<Obj> {
  return new Promise((done) => {
    const chunks: Buffer[] = [];
    request.on("data", (chunk: Buffer) => chunks.push(chunk));
    request.on("end", () => {
      const text = Buffer.concat(chunks).toString("utf8");
      done(text === "" ? {} : (JSON.parse(text) as Obj));
    });
  });
}

export function createKindsFake(send: Send): KindsFake {
  const calls: KindsFake["calls"] = [];
  const kinds: Kind[] = [];
  let nextKind = 1;
  let nextField = 1;

  const fieldsOf = (given: unknown): Field[] =>
    (Array.isArray(given) ? (given as Obj[]) : []).map((f) => ({
      id: nextField++,
      name: String(f.name),
      type: String(f.type),
      options: Array.isArray(f.options) ? (f.options as string[]) : [],
    }));

  const issues = (query: URLSearchParams) => {
    let found = CARDS.filter(
      (card) =>
        (!query.has("project") || String(card.project_id) === query.get("project")) &&
        (!query.has("kind") || card.kind.toLowerCase() === query.get("kind")?.toLowerCase()) &&
        (!query.has("assignee") ||
          card.assignees.some(
            (handle) => handle === query.get("assignee")?.replace(/^@/, "").toLowerCase(),
          )),
    );
    for (const [key, value] of query) {
      if (!key.startsWith("field.")) continue;
      const name = key.slice("field.".length);
      found = found.filter((card) => {
        const have = (card.fields as Obj)[name];
        return have !== undefined && String(have) === value;
      });
    }
    const sort = query.get("sort");
    if (sort) {
      const sign = query.get("order") === "desc" ? -1 : 1;
      found = [...found].sort((a, b) => {
        const x = (a.fields as Obj)[sort];
        const y = (b.fields as Obj)[sort];
        return (x === y ? 0 : (x as number) < (y as number) ? -1 : 1) * sign;
      });
    }
    return found;
  };

  const fake: KindsFake = {
    calls,
    list: () => kinds,
    async handle(request, response, path) {
      if (!/^\/v1\/(kinds(\/\d+)?|issues|projects\/\d+\/kinds)$/.test(path)) return false;
      const method = request.method ?? "GET";
      const url = new URL(request.url ?? "/", "http://x");
      const body = method === "GET" || method === "DELETE" ? {} : await readBody(request);
      calls.push({ method, path: path + url.search, body });
      if (path === "/v1/issues") return json(response, 200, issues(url.searchParams));
      if (path === "/v1/kinds" && method === "GET") return json(response, 200, kinds);
      if (path === "/v1/kinds" && method === "POST") {
        const name = String(body.name ?? "");
        if (kinds.some((k) => k.name.toLowerCase() === name.toLowerCase())) {
          return json(response, 409, `a kind named ${name} exists already`);
        }
        const made: Kind = { id: nextKind++, name, fields: fieldsOf(body.fields), projects: [] };
        kinds.push(made);
        send({ kind: "kind_changed", data: made });
        return json(response, 200, made);
      }
      const project = /^\/v1\/projects\/(\d+)\/kinds$/.exec(path);
      if (project && method === "PUT") {
        const id = Number(project[1]);
        const names = ((body.kinds as string[]) ?? []).map((n) => n.toLowerCase());
        for (const kind of kinds) {
          const on = names.includes(kind.name.toLowerCase());
          kind.projects = kind.projects.filter((p) => p !== id);
          if (on) kind.projects.push(id);
        }
        send({
          kind: "project_kinds_changed",
          data: {
            project: id,
            kinds: kinds.filter((k) => k.projects.includes(id)).map((k) => k.id),
          },
        });
        return json(response, 200, {
          id,
          kinds: kinds.filter((k) => k.projects.includes(id)).map((k) => k.id),
        });
      }
      const one = /^\/v1\/kinds\/(\d+)$/.exec(path);
      const kind = one && kinds.find((k) => k.id === Number(one[1]));
      if (!kind) return json(response, 404, "kind not found");
      if (method === "PUT") {
        kind.name = String(body.name ?? kind.name);
        kind.fields = fieldsOf(body.fields);
        send({ kind: "kind_changed", data: kind });
        return json(response, 200, kind);
      }
      if (method === "DELETE") {
        kinds.splice(kinds.indexOf(kind), 1);
        send({ kind: "kind_removed", data: { kind: kind.id } });
        return json(response, 200, {});
      }
      return json(response, 405, "no such route");
    },
  };
  return fake;
}
