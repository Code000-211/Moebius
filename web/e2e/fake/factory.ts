/**
 * The part of the fake server for the software factory (ADR 0032): a repository's stale settings
 * (`GET` and `PUT /v1/repositories/{id}/upkeep`), a repository's review flow (`/flow`, ADR 0033), a project's backlog and its sync (`/v1/projects/{id}/backlog`), with
 * the JSON the real server sends. The digest item itself is the board fake's (`seedDigest`), since the Inbox reads it
 * from the snapshot.
 */
import type { IncomingMessage, ServerResponse } from "node:http";

export interface FactoryFake {
  /** Answers a route of the factory and returns true; false for any other path. */
  handle(request: IncomingMessage, response: ServerResponse, path: string): Promise<boolean>;
  /** Gives project 1 a backlog, as one that was set up. */
  seedBacklog(): void;
  /** The requests that changed something or synced, in order. */
  readonly requests: Array<{ method: string; path: string; body: unknown }>;
}

interface Upkeep {
  release_after_days: number;
  stale_after_days: number;
  close_after_days: number;
}

interface Flow {
  review_on_green: boolean;
  green_wait_minutes: number;
  unbound_close_days: number;
  status_labels: boolean;
  status_prefix: string;
  work_on_forge: boolean;
  review_round_limit: number;
}

interface Backlog {
  project_id: number;
  repository: string;
  path: string;
  pattern: string;
  closed_marker: string;
  cap: number;
  difficulty_label: string;
  lane_label: string;
}

const PATTERN = String.raw`^#{1,6}\s+(?P<id>[A-Z][A-Z0-9]*-\d+)\s*[:.–—-]?\s*(?P<title>.*?)\s*$`;

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

export function createFactoryFake(): FactoryFake {
  const requests: FactoryFake["requests"] = [];
  const upkeep = new Map<number, Upkeep>();
  const flows = new Map<number, Flow>();
  const flowDefaults = (): Flow => ({
    review_on_green: false,
    green_wait_minutes: 5,
    unbound_close_days: 14,
    status_labels: false,
    status_prefix: "status:",
    work_on_forge: true,
    review_round_limit: 3,
  });
  let backlog: Backlog | null = null;
  const defaults = (): Upkeep => ({
    release_after_days: 14,
    stale_after_days: 14,
    close_after_days: 30,
  });
  return {
    requests,
    seedBacklog() {
      backlog = {
        project_id: 1,
        repository: "demo",
        path: "notes/backlog/optimisation-backlog.md",
        pattern: PATTERN,
        closed_marker: "[closed]",
        cap: 15,
        difficulty_label: "difficulty:",
        lane_label: "lane-required",
      };
    },
    async handle(request, response, path) {
      const method = request.method ?? "GET";
      const stale = /^\/v1\/repositories\/(\d+)\/upkeep$/.exec(path);
      const flow = /^\/v1\/repositories\/(\d+)\/flow$/.exec(path);
      const mine = /^\/v1\/projects\/(\d+)\/backlog(\/sync)?$/.exec(path);
      const templates = /^\/v1\/repositories\/(\d+)\/templates(\/sync)?$/.exec(path);
      if (!stale && !flow && !mine && !templates) return false;
      const body: Record<string, unknown> =
        method === "GET" ? {} : JSON.parse((await readBody(request)) || "{}");
      if (method !== "GET") requests.push({ method, path, body });
      if (templates?.[2]) return reply(response, 200, { change: { id: 31, pull_request: 12 } });
      if (templates) {
        const named = (name: string, first: string, source = "default") => ({
          name,
          source,
          path:
            name === "pull_request"
              ? ".github/PULL_REQUEST_TEMPLATE.md"
              : `.github/moebius/${name}.md`,
          first_line: first,
        });
        return reply(response, 200, {
          templates: [
            named("pull_request", ""),
            named("measurement", "## Measurement"),
            named("done", "/done", "project"),
            named("review-answer", "## Review answer"),
            named("take", "/take"),
            named("release", "/release"),
          ],
          sections: [
            { heading: "Item", required: true, hint: "" },
            { heading: "What the change does", required: true, hint: "" },
            { heading: "Tests I ran", required: true, hint: "" },
            { heading: "Measurement", required: true, hint: "" },
          ],
          problem: null,
        });
      }
      if (flow) {
        const id = Number(flow[1]);
        if (method === "PUT") flows.set(id, { ...(flows.get(id) ?? flowDefaults()), ...body });
        return reply(response, 200, flows.get(id) ?? flowDefaults());
      }
      if (stale) {
        const id = Number(stale[1]);
        if (method === "PUT") upkeep.set(id, { ...(upkeep.get(id) ?? defaults()), ...body });
        return reply(response, 200, upkeep.get(id) ?? defaults());
      }
      const project = Number(mine?.[1]);
      if (mine?.[2]) {
        if (!backlog) return reply(response, 404, "a backlog of the project demo not found");
        const dry = body.dry_run === true;
        return reply(response, 200, {
          project: "demo",
          repository: backlog.repository,
          path: backlog.path,
          dry_run: dry,
          items: 4,
          created: [
            { id: "K-47", number: dry ? 0 : 41, title: "K-47: Cache the compiled plan" },
            { id: "P-16", number: dry ? 0 : 42, title: "P-16: Shorter prompts for the Reviewer" },
          ],
          updated: [{ id: "M-01", number: 12, title: "M-01: Measure the nightly run" }],
          labelled: [],
          closed: [{ id: "K-02", number: 9, title: "K-02: Old cache" }],
          deferred: [],
          unchanged: 0,
          warnings: [],
        });
      }
      if (method === "PUT") {
        backlog = {
          project_id: project,
          repository: "demo",
          path: "",
          pattern: PATTERN,
          closed_marker: "[closed]",
          cap: 15,
          difficulty_label: "difficulty:",
          lane_label: "lane-required",
          ...(backlog ?? {}),
          ...body,
        };
        return reply(response, 200, backlog);
      }
      if (method === "DELETE") {
        backlog = null;
        return reply(response, 200, {});
      }
      return reply(response, 200, backlog);
    },
  };
}
