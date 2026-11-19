/**
 * The part of the fake server for saved views and the Views page: the routes of `docs/reference/desktop-api.md` with
 * the JSON the real server sends. It keeps the views a test makes (a board's "Save view" or the page's own edits) and
 * answers the cards of one from a few canned cards, laid out as the server does: in the view's order, with the open
 * cycle on a card only for the timeline layout. What a person may see is the server's own rule and tested there.
 */
import type { IncomingMessage, ServerResponse } from "node:http";

export interface ViewsFake {
  /** Answers a route of views and returns true; false for any other path. */
  handle(request: IncomingMessage, response: ServerResponse, path: string): Promise<boolean>;
  /** The requests that changed something, in order. */
  readonly requests: Array<{ method: string; path: string; body: unknown }>;
}

interface View {
  id: number;
  name: string;
  owner_id: number;
  shared: boolean;
  filter: Record<string, unknown>;
  layout: string;
  sort: string;
  descending: boolean;
  created_at: number;
  can_change: boolean;
}

const CYCLE = {
  id: 1,
  project_id: 1,
  name: "Cycle 1",
  starts_on: "2026-11-02",
  ends_on: "2026-11-15",
  closed_at: null,
  created_at: 1,
};

/** Cards of the board fake (repository 1), each with the parts a layout sets cards by. */
const CARDS = [
  {
    id: 1,
    number: 1,
    title: "Add a feature",
    column: "In progress",
    due: "2026-12-02",
    cycle: true,
  },
  { id: 2, number: 2, title: "Fix the login redirect", column: "Backlog", due: null, cycle: false },
  {
    id: 3,
    number: 3,
    title: "Write the guide",
    column: "In progress",
    due: "2026-12-01",
    cycle: false,
  },
].map((card) => ({
  repository_id: 1,
  project_id: 1,
  project_key: "DEMO",
  project_number: card.number,
  body: "",
  author: "leon",
  created_at: 1,
  labels: [],
  assignees: [],
  priority: "none",
  ...card,
}));

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

/** A filter as the server answers it: every part, empty when it was not set. */
function whole(filter: Record<string, unknown> | undefined): Record<string, unknown> {
  return {
    projects: [],
    repositories: [],
    labels: [],
    assignees: [],
    priorities: [],
    cycles: [],
    milestones: [],
    columns: [],
    authors: [],
    text: "",
    ...filter,
  };
}

export function createViewsFake(): ViewsFake {
  const requests: ViewsFake["requests"] = [];
  let views: View[] = [
    {
      id: 1,
      name: "Team board",
      owner_id: 2,
      shared: true,
      filter: whole({ columns: ["In progress"] }),
      layout: "board",
      sort: "created",
      descending: false,
      created_at: 1,
      can_change: true,
    },
    {
      id: 2,
      name: "Roadmap",
      owner_id: 3,
      shared: true,
      filter: whole({}),
      layout: "list",
      sort: "created",
      descending: false,
      created_at: 1,
      can_change: false,
    },
  ];
  let nextId = 3;
  const fields = (body: Record<string, unknown>) => ({
    name: typeof body.name === "string" ? body.name : "",
    shared: body.shared === true,
    filter: whole(body.filter as Record<string, unknown> | undefined),
    layout: typeof body.layout === "string" ? body.layout : "list",
    sort: typeof body.sort === "string" ? body.sort : "created",
    descending: body.descending === true,
  });
  const cardsOf = (view: View) => {
    const columns = (view.filter.columns as string[] | undefined) ?? [];
    return CARDS.filter((c) => columns.length === 0 || columns.includes(c.column)).map((c) => ({
      ...c,
      cycle: view.layout === "timeline" && c.cycle ? CYCLE : null,
    }));
  };

  return {
    requests,
    async handle(request, response, path) {
      const method = request.method ?? "GET";
      const one = /^\/v1\/views\/(\d+)(\/cards)?$/.exec(path);
      if (path !== "/v1/views" && !one) return false;
      const body: Record<string, unknown> =
        method === "GET" ? {} : JSON.parse((await readBody(request)) || "{}");
      if (method !== "GET") requests.push({ method, path, body });
      if (path === "/v1/views" && method === "GET") return reply(response, 200, views);
      if (path === "/v1/views" && method === "POST") {
        const made = {
          id: nextId++,
          owner_id: 1,
          created_at: 2,
          can_change: true,
          ...fields(body),
        };
        views = [...views, made];
        return reply(response, 200, made);
      }
      if (!one) return false;
      const view = views.find((v) => v.id === Number(one[1]));
      if (!view) return reply(response, 404, `view ${one[1]} not found`);
      if (one[2] && method === "GET") return reply(response, 200, { view, cards: cardsOf(view) });
      if (method === "GET") return reply(response, 200, view);
      if (method === "PATCH") {
        // A field left out stays as it was, as on the server.
        const given = fields({ ...view, ...body });
        Object.assign(view, given);
        return reply(response, 200, view);
      }
      if (method === "DELETE") {
        views = views.filter((v) => v !== view);
        return reply(response, 200, view);
      }
      return false;
    },
  };
}
