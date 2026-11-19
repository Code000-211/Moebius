/**
 * The part of the fake server for the Milestones page and a card's Milestone property: the routes of
 * `docs/reference/desktop-api.md` with the JSON the real server sends. Two projects hold cards: the board's own
 * (project 1, cards 1 and 2 of the board fake) and Storefront (project 100, cards that only milestones know). Progress
 * is counted from the cards of the projects that are not hidden, as the server does for a person who may not see a
 * private project: `hide` makes the fake leave one out of every count and list.
 */
import type { IncomingMessage, ServerResponse } from "node:http";

type Send = (event: { kind: string; data: unknown }) => void;

export interface MilestonesFake {
  /** Answers a route of milestones and returns true; false for any other path. */
  handle(request: IncomingMessage, response: ServerResponse, path: string): Promise<boolean>;
  /** Leaves the cards of a project out of every count and list, as for a person who is not its member. */
  hide(project: number): void;
  /** The requests that changed something, in order. */
  readonly requests: Array<{ method: string; path: string; body: unknown }>;
}

interface Card {
  id: number;
  project_id: number;
  repository_id: number;
  number: number;
  title: string;
  column: string;
  done: boolean;
}

interface Milestone {
  id: number;
  name: string;
  target_date: string;
  description: string;
  created_at: number;
}

const PROJECTS: Record<number, { name: string; key: string }> = {
  1: { name: "demo", key: "DEMO" },
  100: { name: "Storefront", key: "SHOP" },
};

/** The cards milestones can hold; 1 and 2 are cards of the board fake. */
const CARDS: Card[] = [
  {
    id: 1,
    project_id: 1,
    repository_id: 1,
    number: 1,
    title: "Add a feature",
    column: "In progress",
    done: false,
  },
  {
    id: 2,
    project_id: 1,
    repository_id: 1,
    number: 2,
    title: "Fix the login redirect",
    column: "Backlog",
    done: false,
  },
  {
    id: 9001,
    project_id: 100,
    repository_id: 100,
    number: 1,
    title: "Take payments",
    column: "Done",
    done: true,
  },
  {
    id: 9002,
    project_id: 100,
    repository_id: 100,
    number: 2,
    title: "Refund an order",
    column: "Done",
    done: true,
  },
  {
    id: 9003,
    project_id: 100,
    repository_id: 100,
    number: 3,
    title: "Print a receipt",
    column: "In progress",
    done: false,
  },
];

/** 2026-10-01, in seconds. */
const MADE = Date.UTC(2026, 9, 1) / 1000;

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

export function createMilestonesFake(send: Send): MilestonesFake {
  let milestones: Milestone[] = [
    {
      id: 1,
      name: "Launch 2.0",
      target_date: "2030-12-12",
      description: "Everything for the launch.",
      created_at: MADE,
    },
    { id: 2, name: "Payments v2", target_date: "2031-01-20", description: "", created_at: MADE },
    { id: 3, name: "Annual report", target_date: "2031-02-28", description: "", created_at: MADE },
  ];
  /** Which cards each milestone holds. */
  const held = new Map<number, Set<number>>([
    [1, new Set([1, 2, 9001, 9002])],
    [2, new Set([9001, 9002, 9003])],
    [3, new Set([2])],
  ]);
  const hidden = new Set<number>();
  const requests: Array<{ method: string; path: string; body: unknown }> = [];
  let nextId = 4;

  const visible = (id: number) =>
    (held.get(id) ? [...(held.get(id) as Set<number>)] : [])
      .map((card) => CARDS.find((each) => each.id === card))
      .filter((card): card is Card => card !== undefined && !hidden.has(card.project_id));
  const view = (m: Milestone) => {
    const cards = visible(m.id);
    const projects = Object.entries(PROJECTS)
      .map(([id, p]) => {
        const of = cards.filter((card) => card.project_id === Number(id));
        return {
          id: Number(id),
          ...p,
          cards: of.length,
          done: of.filter((card) => card.done).length,
        };
      })
      .filter((p) => p.cards > 0);
    return { ...m, cards: cards.length, done: cards.filter((card) => card.done).length, projects };
  };
  const sorted = () =>
    [...milestones].sort((a, b) => a.target_date.localeCompare(b.target_date) || a.id - b.id);
  const fields = (body: Record<string, unknown>) => ({
    name: typeof body.name === "string" ? body.name : "",
    target_date: typeof body.target_date === "string" ? body.target_date : "",
    description: typeof body.description === "string" ? body.description : "",
  });
  const refused = (id: number | null, f: ReturnType<typeof fields>) => {
    if (f.name === "" || f.name.trim() !== f.name) return `"${f.name}" is not a milestone name`;
    if (!/^\d{4}-\d{2}-\d{2}$/.test(f.target_date))
      return `"${f.target_date}" is not a target date`;
    if (milestones.some((m) => m.id !== id && m.name.toLowerCase() === f.name.toLowerCase())) {
      return `a milestone named ${f.name} exists already`;
    }
    return null;
  };

  return {
    requests,
    hide(project) {
      hidden.add(project);
    },
    async handle(request, response, path) {
      const method = request.method ?? "GET";
      const one = /^\/v1\/milestones\/(\d+)$/.exec(path);
      const ofCard = /^\/v1\/cards\/(\d+)\/milestones$/.exec(path);
      if (path !== "/v1/milestones" && !one && !ofCard) return false;
      const body: Record<string, unknown> =
        method === "GET" ? {} : JSON.parse((await readBody(request)) || "{}");
      if (method !== "GET") requests.push({ method, path, body });

      if (path === "/v1/milestones" && method === "GET")
        return reply(response, 200, sorted().map(view));
      if (path === "/v1/milestones" && method === "POST") {
        const f = fields(body);
        const problem = refused(null, f);
        if (problem) return reply(response, problem.includes("exists") ? 409 : 422, problem);
        const made = { id: nextId++, ...f, created_at: Math.floor(Date.now() / 1000) };
        milestones = [...milestones, made];
        held.set(made.id, new Set());
        send({ kind: "milestone_changed", data: made });
        return reply(response, 200, made);
      }
      if (one) {
        const id = Number(one[1]);
        const milestone = milestones.find((m) => m.id === id);
        if (!milestone) return reply(response, 404, `milestone ${id} not found`);
        if (method === "GET")
          return reply(response, 200, { ...view(milestone), cards: visible(id) });
        if (method === "PATCH") {
          const f = fields(body);
          const problem = refused(id, f);
          if (problem) return reply(response, problem.includes("exists") ? 409 : 422, problem);
          Object.assign(milestone, f);
          send({ kind: "milestone_changed", data: milestone });
          return reply(response, 200, milestone);
        }
        if (method === "DELETE") {
          milestones = milestones.filter((m) => m.id !== id);
          held.delete(id);
          send({ kind: "milestone_removed", data: { milestone: id } });
          return reply(response, 200, milestone);
        }
      }
      if (ofCard) {
        const card = Number(ofCard[1]);
        const of = () => sorted().filter((m) => held.get(m.id)?.has(card));
        if (method === "GET") return reply(response, 200, of());
        if (method === "PUT") {
          const ids = Array.isArray(body.milestones) ? (body.milestones as number[]) : [];
          if (ids.some((id) => !milestones.some((m) => m.id === id))) {
            return reply(response, 404, "milestone not found");
          }
          for (const [id, cards] of held) {
            if (ids.includes(id)) cards.add(card);
            else cards.delete(card);
          }
          send({
            kind: "card_milestones_changed",
            data: { card, milestones: of().map((m) => m.id) },
          });
          return reply(response, 200, of());
        }
      }
      return false;
    },
  };
}
