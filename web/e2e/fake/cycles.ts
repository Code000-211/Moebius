/**
 * The part of the fake server for the Cycles page and a card's Cycle property: the routes of
 * `docs/reference/desktop-api.md` with the JSON the real server sends. Project 1 (the board's own) holds two cycles,
 * "Cycle 1" with cards 1 and 2 (card 1 done, 5 and 3 points) and an empty "Cycle 2" after it. The rules are the
 * server's own and tested there: a card is in at most one open cycle, a closed cycle keeps its cards, closing carries
 * the cards that are not done to the chosen cycle or out.
 */
import type { IncomingMessage, ServerResponse } from "node:http";

type Send = (event: { kind: string; data: unknown }) => void;

export interface CyclesFake {
  /** Answers a route of cycles and returns true; false for any other path. */
  handle(request: IncomingMessage, response: ServerResponse, path: string): Promise<boolean>;
  /** The requests that changed something, in order. */
  readonly requests: Array<{ method: string; path: string; body: unknown }>;
}

interface Cycle {
  id: number;
  project_id: number;
  name: string;
  starts_on: string;
  ends_on: string;
  closed_at: number | null;
  created_at: number;
}

/** The cards cycles can hold: 1 and 2 are cards of the board fake. */
const CARDS = [
  { id: 1, number: 1, title: "Add a feature", column: "Done", done: true, estimate: 5 },
  {
    id: 2,
    number: 2,
    title: "Fix the login redirect",
    column: "In progress",
    done: false,
    estimate: 3,
  },
];

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

export function createCyclesFake(send: Send): CyclesFake {
  const MADE = Date.UTC(2026, 9, 1) / 1000;
  let cycles: Cycle[] = [
    {
      id: 1,
      project_id: 1,
      name: "Cycle 1",
      starts_on: "2026-11-02",
      ends_on: "2026-11-15",
      closed_at: null,
      created_at: MADE,
    },
    {
      id: 2,
      project_id: 1,
      name: "Cycle 2",
      starts_on: "2026-11-16",
      ends_on: "2026-11-29",
      closed_at: null,
      created_at: MADE,
    },
  ];
  /** Which cards each cycle holds, closed ones included. */
  const held = new Map<number, Set<number>>([
    [1, new Set([1, 2])],
    [2, new Set()],
  ]);
  const requests: Array<{ method: string; path: string; body: unknown }> = [];
  let nextId = 3;

  const cardsOf = (id: number) =>
    [...(held.get(id) ?? [])]
      .map((card) => CARDS.find((each) => each.id === card))
      .filter((card) => card !== undefined);
  const view = (cycle: Cycle) => {
    const cards = cardsOf(cycle.id);
    const points = (only: boolean) =>
      cards.filter((c) => c.done || !only).reduce((sum, c) => sum + c.estimate, 0);
    return {
      ...cycle,
      cards: cards.length,
      done: cards.filter((c) => c.done).length,
      points: points(false),
      points_done: points(true),
    };
  };
  const detail = (cycle: Cycle) => {
    const { cards: _count, ...rest } = view(cycle);
    const cards = cardsOf(cycle.id).map((c) => ({
      id: c.id,
      repository_id: 1,
      number: c.number,
      title: c.title,
      column: c.column,
      done: c.done,
      estimate: c.estimate,
      added_at: MADE,
      done_at: c.done ? MADE : null,
      by_agent: c.done,
      tokens: c.done ? 5000 : 0,
    }));
    const done = rest.points_done;
    const burn = [
      { day: cycle.starts_on, scope: rest.points, done: 0 },
      { day: "2026-11-03", scope: rest.points, done },
    ];
    return {
      ...rest,
      cards,
      burn_up: burn,
      figures: {
        by_agents: cards.filter((c) => c.done && c.by_agent).length,
        tokens: cards.reduce((sum, c) => sum + c.tokens, 0),
        tokens_per_point:
          done > 0 ? Math.floor(cards.reduce((s, c) => s + c.tokens, 0) / done) : null,
      },
    };
  };
  const open = (card: number) =>
    cycles.find((c) => c.closed_at === null && held.get(c.id)?.has(card));
  const fields = (body: Record<string, unknown>) => ({
    name: typeof body.name === "string" ? body.name : "",
    starts_on: typeof body.starts_on === "string" ? body.starts_on : "",
    ends_on: typeof body.ends_on === "string" ? body.ends_on : "",
  });
  const refused = (id: number | null, f: ReturnType<typeof fields>) => {
    if (f.name === "" || f.name.trim() !== f.name) return `"${f.name}" is not a cycle name`;
    if (!/^\d{4}-\d{2}-\d{2}$/.test(f.starts_on) || !/^\d{4}-\d{2}-\d{2}$/.test(f.ends_on))
      return "a cycle's dates are written YYYY-MM-DD";
    if (f.ends_on < f.starts_on) return `a cycle cannot end (${f.ends_on}) before it starts`;
    if (cycles.some((c) => c.id !== id && c.name.toLowerCase() === f.name.toLowerCase()))
      return `a cycle named ${f.name} exists already`;
    return null;
  };

  return {
    requests,
    async handle(request, response, path) {
      const method = request.method ?? "GET";
      const ofProject = /^\/v1\/projects\/(\d+)\/cycles$/.exec(path);
      const one = /^\/v1\/cycles\/(\d+)$/.exec(path);
      const close = /^\/v1\/cycles\/(\d+)\/close$/.exec(path);
      const ofCard = /^\/v1\/cards\/(\d+)\/cycle$/.exec(path);
      if (!ofProject && !one && !close && !ofCard) return false;
      const body: Record<string, unknown> =
        method === "GET" ? {} : JSON.parse((await readBody(request)) || "{}");
      if (method !== "GET") requests.push({ method, path, body });

      if (ofProject) {
        const project = Number(ofProject[1]);
        if (project !== 1) return reply(response, 404, `project ${project} not found`);
        if (method === "GET") {
          const sorted = cycles
            .filter((c) => c.project_id === project)
            .sort((a, b) => b.starts_on.localeCompare(a.starts_on) || b.id - a.id);
          return reply(response, 200, sorted.map(view));
        }
        const f = fields(body);
        const problem = refused(null, f);
        if (problem) return reply(response, problem.includes("exists") ? 409 : 422, problem);
        const made = { id: nextId++, project_id: project, ...f, closed_at: null, created_at: MADE };
        cycles = [...cycles, made];
        held.set(made.id, new Set());
        send({ kind: "cycle_changed", data: made });
        return reply(response, 200, made);
      }
      if (close && method === "POST") {
        const cycle = cycles.find((c) => c.id === Number(close[1]));
        if (!cycle) return reply(response, 404, "cycle not found");
        if (cycle.closed_at !== null)
          return reply(response, 422, `${cycle.name} is already closed`);
        let target: Cycle | undefined;
        if (body.carry === "next") {
          target =
            cycles.find((c) => c.id === body.to) ??
            cycles
              .filter((c) => c.closed_at === null && c.starts_on > cycle.starts_on)
              .sort((a, b) => a.starts_on.localeCompare(b.starts_on))[0];
          if (!target || target.id === cycle.id || target.closed_at !== null)
            return reply(response, 422, "no open cycle to carry the cards to");
        }
        cycle.closed_at = Math.floor(Date.now() / 1000);
        const carried = cardsOf(cycle.id)
          .filter((c) => !c.done)
          .map((c) => c.id);
        if (target) for (const card of carried) held.get(target.id)?.add(card);
        send({ kind: "cycle_changed", data: cycle });
        for (const card of carried)
          send({ kind: "card_cycle_changed", data: { card, cycle: target?.id ?? null } });
        return reply(response, 200, { cycle, carried_to: target ?? null, carried });
      }
      if (one) {
        const id = Number(one[1]);
        const cycle = cycles.find((c) => c.id === id);
        if (!cycle) return reply(response, 404, `cycle ${id} not found`);
        if (method === "GET") return reply(response, 200, detail(cycle));
        if (method === "PATCH") {
          const f = fields(body);
          const problem = refused(id, f);
          if (problem) return reply(response, problem.includes("exists") ? 409 : 422, problem);
          Object.assign(cycle, f);
          send({ kind: "cycle_changed", data: cycle });
          return reply(response, 200, cycle);
        }
        if (method === "DELETE") {
          cycles = cycles.filter((c) => c.id !== id);
          held.delete(id);
          send({ kind: "cycle_removed", data: { cycle: id } });
          return reply(response, 200, cycle);
        }
      }
      if (ofCard) {
        const card = Number(ofCard[1]);
        if (method === "GET") return reply(response, 200, open(card) ?? null);
        if (method === "PUT") {
          const id = body.cycle;
          const target = typeof id === "number" ? cycles.find((c) => c.id === id) : undefined;
          if (typeof id === "number" && !target) return reply(response, 404, "cycle not found");
          if (target && target.closed_at !== null)
            return reply(response, 422, `${target.name} is closed`);
          for (const c of cycles) if (c.closed_at === null) held.get(c.id)?.delete(card);
          if (target) held.get(target.id)?.add(card);
          send({ kind: "card_cycle_changed", data: { card, cycle: target?.id ?? null } });
          return reply(response, 200, target ?? null);
        }
      }
      return false;
    },
  };
}
