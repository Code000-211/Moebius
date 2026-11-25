/**
 * The part of the fake server for the test lane (ADR 0035): a repository's lane (`GET` and `PUT
 * /v1/repositories/{id}/lane`), the lanes (`GET /v1/lanes`, `POST /v1/lanes/{name}/resume`) and a card's report
 * (`GET /v1/cards/{id}/lane`), with the JSON the real server sends. `seed` chooses what the lanes are like, and
 * `seedCard` gives a card a report.
 */
import type { IncomingMessage, ServerResponse } from "node:http";

type Isolation = "always" | "external_only" | "off";

export interface LaneFake {
  /** Answers a route of the lane and returns true; false for any other path. */
  handle(request: IncomingMessage, response: ServerResponse, path: string): Promise<boolean>;
  /** The server's lanes: `gpu1` with the isolation given, and `gpu2`, which is the server itself and is not isolated. */
  seed(options?: { isolation?: Isolation; assigned?: boolean; paused?: boolean }): void;
  /** Gives the card the report of a job at its head: measured, failed or running. */
  seedCard(card: number, state: "measured" | "failed" | "running"): void;
  /** The requests that changed something, in order. */
  readonly requests: Array<{ method: string; path: string; body: unknown }>;
}

const HEAD = "d34db33f0123456789abcdef0123456789abcdef";
const BASE = "0123456789abcdef0123456789abcdef01234567";

function warningOf(isolation: Isolation): string | null {
  if (isolation === "always") return null;
  return isolation === "off"
    ? "Every job runs directly on the machine, with the machine's files, network and credentials of the account they run as, including the code of pull requests someone else opened."
    : "Jobs for the server's own changes run directly on the machine, with the machine's files, network and credentials of the account they run as. Only jobs for pull requests someone else opened run in a container.";
}

function lane(name: string, isolation: Isolation, paused: boolean) {
  const own = name === "gpu1";
  return {
    name,
    runner: own ? "gpu-box" : null,
    isolation,
    warning: warningOf(isolation),
    runtime: "podman",
    image: isolation === "off" ? null : "rocm/dev-ubuntu-24.04:6.2",
    gpus: own ? "2x MI300X" : "1x W7900",
    power_cap: own ? "750 W" : "230 W",
    rocm: "6.2.1",
    driver: null,
    devices: ["/dev/kfd", "/dev/dri"],
    timeout_seconds: 7200,
    queued: own ? 1 : 0,
    running: null,
    paused: paused
      ? {
          since: 1790000000,
          reason:
            "3 jobs in a row ended because of the lane itself; the last: the container runtime podman is missing on the machine",
        }
      : null,
  };
}

const MACHINE = {
  lane: "gpu1",
  where: "gpu-box",
  gpus: "2x MI300X",
  power_cap: "750 W",
  rocm: "6.2.1",
  driver: null,
  image: "rocm/dev-ubuntu-24.04:6.2",
  isolation: "always",
  contained: true,
};

function job(card: number, state: "measured" | "failed" | "running") {
  const base = {
    id: card,
    repository_id: 1,
    card_id: card,
    change_id: 40,
    lane: "gpu1",
    head: HEAD,
    base: BASE,
    external: true,
    state,
    failed_step: null as string | null,
    summary: "",
    step: null as string | null,
    delivered: state !== "running",
    queued_at: 1790000000,
    started_at: 1790000010,
    ended_at: state === "running" ? null : 1790000900,
  };
  if (state === "running") {
    return { ...base, step: "measure (after, run 2 of 3)", report: null };
  }
  if (state === "failed") {
    return {
      ...base,
      failed_step: "gpu selftests",
      summary: "the step gpu selftests failed: it exited with code 3",
      report: {
        machine: MACHINE,
        steps: [
          { name: "build", state: "passed", seconds: 212 },
          { name: "card-free tests", state: "passed", seconds: 41 },
          { name: "gpu selftests", state: "failed", seconds: 18 },
        ],
        failure: {
          step: "gpu selftests",
          why: "it exited with code 3",
          output:
            "selftest attention_fused ... ok\nselftest paged_decode ... FAILED\nmax abs error 0.0213 > 0.01\nerror: 1 selftest failed",
        },
      },
    };
  }
  return {
    ...base,
    summary: "every step passed and 2 measured rows came of it",
    report: {
      machine: MACHINE,
      steps: [
        { name: "build", state: "passed", seconds: 212 },
        { name: "card-free tests", state: "passed", seconds: 41 },
        { name: "gpu selftests", state: "passed", seconds: 96 },
        { name: "measure", state: "passed", seconds: 604 },
      ],
      measurements: [
        {
          step: "measure",
          runs: 3,
          parity: "identical (out/logits.bin, every run)",
          rows: [
            {
              name: "decode_tps",
              unit: "tok/s",
              before: [40.1, 41, 40.6],
              after: [44.2, 44.9, 44.5],
              median_before: 40.6,
              median_after: 44.5,
              change_percent: 9.6059,
            },
            {
              name: "prefill_tps",
              unit: "tok/s",
              before: [1210, 1198, 1204],
              after: [1207, 1211, 1203],
              median_before: 1204,
              median_after: 1207,
              change_percent: 0.2492,
            },
          ],
        },
      ],
    },
  };
}

function reply(response: ServerResponse, status: number, body: unknown): true {
  const text = typeof body === "string";
  response.writeHead(status, { "content-type": text ? "text/plain" : "application/json" });
  response.end(text ? body : JSON.stringify(body));
  return true;
}

function readBody(request: IncomingMessage): Promise<string> {
  return new Promise((done) => {
    const chunks: Buffer[] = [];
    request.on("data", (chunk: Buffer) => chunks.push(chunk));
    request.on("end", () => done(Buffer.concat(chunks).toString("utf8")));
  });
}

export function createLaneFake(): LaneFake {
  const requests: LaneFake["requests"] = [];
  let isolation: Isolation = "always";
  let paused = false;
  let seeded = false;
  const assigned = new Map<number, string | null>();
  const reports = new Map<number, ReturnType<typeof job>>();
  const lanes = () => (seeded ? [lane("gpu1", isolation, paused), lane("gpu2", "off", false)] : []);
  const view = (repository: number) => {
    const name = assigned.get(repository) ?? null;
    const all = lanes();
    const current = all.find((l) => l.name === name) ?? null;
    const jobs = [...reports.values()].filter((j) => j.repository_id === repository);
    return {
      lane: name,
      current,
      lanes: all,
      definition: {
        present: true,
        problem: null,
        steps: ["build", "card-free tests", "gpu selftests", "measure"],
        measures: true,
      },
      jobs: jobs.reverse().slice(0, 10),
    };
  };
  return {
    requests,
    seed(options = {}) {
      seeded = true;
      isolation = options.isolation ?? "always";
      paused = options.paused ?? false;
      if (options.assigned ?? true) assigned.set(1, "gpu1");
    },
    seedCard(card, state) {
      reports.set(card, job(card, state));
    },
    async handle(request, response, path) {
      const method = request.method ?? "GET";
      const repository = /^\/v1\/repositories\/(\d+)\/lane$/.exec(path);
      const card = /^\/v1\/cards\/(\d+)\/lane$/.exec(path);
      const resume = /^\/v1\/lanes\/([^/]+)\/resume$/.exec(path);
      if (!repository && !card && !resume && path !== "/v1/lanes") return false;
      if (path === "/v1/lanes") return reply(response, 200, lanes());
      if (resume) {
        requests.push({ method, path, body: {} });
        paused = false;
        return reply(response, 200, lanes());
      }
      if (repository) {
        const id = Number(repository[1]);
        if (method === "PUT") {
          const body = JSON.parse((await readBody(request)) || "{}") as { lane: string | null };
          requests.push({ method, path, body });
          if (body.lane && !lanes().some((l) => l.name === body.lane)) {
            return reply(response, 422, `"${body.lane}" is no lane of the server's configuration`);
          }
          assigned.set(id, body.lane || null);
        }
        return reply(response, 200, view(id));
      }
      const id = Number(card?.[1]);
      const found = reports.get(id);
      return reply(response, 200, {
        lane: found ? "gpu1" : null,
        jobs: found ? [found] : [],
        head: found ? HEAD : null,
        waiting: null,
      });
    },
  };
}
