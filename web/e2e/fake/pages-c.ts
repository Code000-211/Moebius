/**
 * The part of the fake server for the pages of Rules, Automations, Retro, Experiments and Decision model, and for the
 * script editor of the board: the data and the routes of `docs/reference/desktop-api.md`, with the JSON the real server
 * sends (the types of `crates/moebius-desktop/src/client/`). The state changes the way the real one does: saving a
 * rules file or a script checks it first and answers 422 with a sentence, saving an automation checks its schedule,
 * "Run now" adds a firing and sends `automation_fired`, a research moves from draft to confirmed to running to
 * stopped, an export is added, a takeover released. A test can make the next request to a route fail (`refuseNext`)
 * and read the requests that changed something (`requests`).
 */
import type { IncomingMessage, ServerResponse } from "node:http";

const NOW = () => Math.floor(Date.now() / 1000);
/** A quarter of an hour over a whole number of hours, so that `in 3 h` stays true while a test runs. */
const IN = (hours: number) => NOW() + hours * 3600 + 900;

export interface PagesCFake {
  /** Answers a route of these pages and returns true; false for any other path. */
  handle(request: IncomingMessage, response: ServerResponse, path: string): Promise<boolean>;
  /** The requests that changed something, in order. */
  readonly requests: Array<{ method: string; path: string; body: unknown }>;
  /** Makes the next request with `method` to `path` fail with `status` and `message`. */
  refuseNext(method: string, path: string, status: number, message: string): void;
  /** The text of the script of a column (lower case), once saved. */
  readonly scripts: Map<string, string>;
  /** What `GET /v1/research` lists; a test that needs a server with no research empties it. */
  readonly research: Array<Json & { id: number; status: string }>;
  /** What `GET /v1/retros` lists. A test that needs more days than the four writes them here. */
  readonly days: Array<{ date: string; status: string; errors: number; proposals: number }>;
}

type Json = Record<string, unknown>;

function readBody(request: IncomingMessage): Promise<string> {
  return new Promise((done) => {
    const chunks: Buffer[] = [];
    request.on("data", (chunk: Buffer) => chunks.push(chunk));
    request.on("end", () => done(Buffer.concat(chunks).toString("utf8")));
  });
}

function reply(response: ServerResponse, status: number, body: unknown): true {
  if (typeof body === "string") {
    response.writeHead(status, { "content-type": "text/plain" });
    response.end(body);
  } else {
    response.writeHead(status, { "content-type": "application/json" });
    response.end(JSON.stringify(body));
  }
  return true;
}

/** The rules of a rules file: the `## name` sections, and a sentence when a name is missing or given twice. */
export function parseRules(
  text: string,
): { rules: Array<{ name: string; text: string }> } | string {
  const rules: Array<{ name: string; lines: string[] }> = [];
  const lines = text.split("\n");
  for (const [index, line] of lines.entries()) {
    if (line.startsWith("## ")) {
      const name = line.slice(3).trim();
      if (name === "") return `line ${index + 1}: a rule needs a name`;
      if (rules.some((rule) => rule.name === name)) {
        return `line ${index + 1}: the rule "${name}" is named twice`;
      }
      rules.push({ name, lines: [] });
    } else {
      rules.at(-1)?.lines.push(line);
    }
  }
  return { rules: rules.map((rule) => ({ name: rule.name, text: rule.lines.join("\n").trim() })) };
}

/** The path as a person writes it: a segment with a space arrives from the browser as `%20`. */
function decoded(path: string): string {
  try {
    return decodeURIComponent(path);
  } catch {
    return path;
  }
}

const SCRIPT_FILES: Record<string, string> = {
  backlog: "backlog",
  // The column is "Not started"; its file in a repository keeps the name it has always had.
  "not started": "ready",
  "in progress": "code",
  "in review": "review",
  "ready to merge": "merge",
  done: "done",
};

export function createPagesCFake(
  broadcast: (event: { kind: string; data: unknown }) => void,
): PagesCFake {
  const requests: PagesCFake["requests"] = [];
  const refusals = new Map<string, { status: number; message: string }>();
  const scripts = new Map<string, string>();

  // The rules.
  let proseText =
    "Rules for every run.\n\n## leave-ci-alone\nDo not edit files under .github/ or any other CI configuration.\n\n## no-vendoring\nDo not copy dependencies into the tree.\n";
  let repositoryText = "## keep-migrations\nDo not edit a migration that exists.";
  const rulesView = () => {
    const prose = parseRules(proseText) as { rules: Json[] };
    const repository = parseRules(repositoryText) as { rules: Json[] };
    return {
      patterns: [
        {
          name: "no-force-push",
          description: "git push --force rewrites history on the remote",
        },
        {
          name: "stay-in-the-worktree",
          description: "files are written only inside the run's worktree",
        },
      ],
      prose: {
        source: "harness",
        editable: true,
        text: proseText,
        rules: prose.rules,
        error: null,
      },
      repositories: [
        {
          repository: "demo",
          repository_id: 1,
          text: repositoryText,
          rules: repository.rules,
          error: null,
        },
      ],
      refusals: { "no-force-push": 3, "leave-ci-alone": 1 },
    };
  };

  // The automations.
  interface Firing {
    id: number;
    status: string;
    run_id: number | null;
    chat_status: string | null;
    manual: boolean;
    reason: string;
    at: number;
  }
  let nextAutomation = 3;
  let nextFiring = 5;
  const automations: Array<Json & { id: number; enabled: boolean; schedule: string }> = [
    {
      id: 1,
      name: "Morning triage",
      schedule: "0 8 * * 1-5",
      agent: "Moebius",
      prompt:
        "Triage the issues opened since yesterday: label them and say which are ready to start.",
      repository_id: 1,
      harness_repository: false,
      enabled: true,
      next_firing: IN(3),
      last_firing: null,
    },
    {
      id: 2,
      name: "Outdated dependencies",
      schedule: "0 9 * * 5",
      agent: "Coder",
      prompt: "Look for outdated dependencies and offer an upgrade.",
      repository_id: null,
      harness_repository: false,
      enabled: true,
      next_firing: IN(48),
      last_firing: {
        id: 2,
        status: "failed",
        run_id: null,
        chat_status: null,
        manual: false,
        reason: "the agent Coder cannot run: its model is not served",
        at: NOW() - 5 * 86_400,
      },
    },
  ];
  const firings = new Map<number, Firing[]>([
    [
      1,
      [
        {
          id: 4,
          status: "skipped",
          run_id: null,
          chat_status: null,
          manual: false,
          reason: "the chat of the firing before (41) still runs",
          at: NOW() - 3 * 3600,
        },
        {
          id: 3,
          status: "started",
          run_id: 41,
          chat_status: "succeeded",
          manual: false,
          reason: "",
          at: NOW() - 21 * 3600,
        },
      ],
    ],
  ]);
  const checkedAutomation = (body: Json): string | null => {
    if (typeof body.name !== "string" || body.name.trim() === "") return "the name is blank";
    if (typeof body.prompt !== "string" || body.prompt.trim() === "") return "the prompt is blank";
    if (
      String(body.schedule ?? "")
        .trim()
        .split(/\s+/).length !== 5
    ) {
      return "the schedule must have five fields";
    }
    return null;
  };

  // The Retro.
  const today = new Date().toISOString().slice(0, 10);
  const retroDays: Array<{ date: string; status: string; errors: number; proposals: number }> = [
    { date: "2026-10-28", status: "proposed", errors: 6, proposals: 3 },
    { date: "2026-10-27", status: "reported", errors: 0, proposals: 0 },
    { date: "2026-10-26", status: "failed", errors: 3, proposals: 0 },
    { date: "2026-10-25", status: "proposed", errors: 1, proposals: 2 },
  ];
  const proposals = [
    {
      id: 1,
      kind: "harness",
      retro_date: "2026-10-28",
      branch: "proposal/2026-10-28-small-commits",
      title: "Tell the Coder to run the tests first",
      body: "The Coder's changes failed the checks twice on 2026-10-28; its instructions now say to run the tests before it stops.",
      agents: ["Coder"],
      cards: [48, 50],
      status: "positive",
      trials: [
        {
          id: 1,
          proposal_id: 1,
          card_id: 48,
          regression: false,
          run_id: 30,
          outcome: "positive",
          reason: "the run succeeded and the checks pass",
          measurements: {},
          at: NOW(),
        },
        {
          id: 2,
          proposal_id: 1,
          card_id: 50,
          regression: false,
          run_id: 31,
          outcome: "positive",
          reason: "the run succeeded, run_failed did not happen again",
          measurements: {},
          at: NOW(),
        },
      ],
      diff: "",
      edits: {},
      repository: null,
      change: null,
      at: NOW(),
    },
    {
      id: 2,
      kind: "settings",
      retro_date: "2026-10-28",
      branch: "proposal/2026-10-28-round-limit",
      title: "Raise the round limit",
      body: "Two cards hit the limit of five rounds.",
      agents: [],
      cards: [],
      status: "untried",
      trials: [
        {
          id: 3,
          proposal_id: 2,
          card_id: null,
          regression: false,
          run_id: null,
          outcome: "skipped",
          reason: "the proposal names no cards, so there is no task to try it on",
          measurements: {},
          at: NOW(),
        },
      ],
      diff: "--- a/moebius.toml\n+++ b/moebius.toml\n@@ -1 +1 @@\n-round_limit = 5\n+round_limit = 8\n",
      edits: { set: { round_limit: 8 }, remove: [] },
      repository: null,
      change: null,
      at: NOW(),
    },
    // The two the Inbox holds items for (`fake/pages-a.ts`), from an earlier day: one for the harness, whose change
    // is a branch rather than a diff, and one for `moebius.toml`, whose change comes as a diff.
    {
      id: 4,
      kind: "harness",
      retro_date: "2026-10-25",
      branch: "proposal/2026-10-25-read-tests",
      title: "Let the Coder read the tests first",
      body: "Two of the Coder's changes on 2026-10-25 failed a test that was already in the repository; its instructions now say to read the tests a change has to pass before writing it.",
      agents: ["Coder"],
      cards: [3],
      status: "positive",
      trials: [],
      diff: "",
      edits: {},
      repository: null,
      change: null,
      at: NOW(),
    },
    {
      id: 5,
      kind: "settings",
      retro_date: "2026-10-25",
      branch: "proposal/2026-10-25-round-limit",
      title: "Raise the round limit to 7",
      body: "Cards ran out of rounds before the review passed.",
      agents: [],
      cards: [],
      status: "untried",
      trials: [],
      diff: "--- a/moebius.toml\n+++ b/moebius.toml\n@@ -1 +1 @@\n-round_limit = 5\n+round_limit = 7\n",
      edits: { set: { round_limit: 7 }, remove: [] },
      repository: null,
      change: null,
      at: NOW(),
    },
  ];
  const report = {
    errors: [
      {
        kind: "tool_failed",
        run: 12,
        card: 48,
        agent: "Coder",
        text: "bash: cargo test exited with 101",
      },
      {
        kind: "tool_refused",
        run: 12,
        card: 48,
        agent: "Coder",
        text: "bash: no-destructive-commands: rm -rf target",
      },
      { kind: "run_failed", run: 14, card: 50, agent: "Coder", text: "the agent changed nothing" },
      {
        kind: "checks_failed",
        run: null,
        card: 48,
        agent: null,
        text: "The checks failed:\ncargo test",
      },
    ],
  };
  let lessons: Array<Json & { id: number }> = [
    {
      id: 3,
      repository_id: 1,
      kind: "rejection",
      situation: "Changing the tokenizer",
      lesson: "The Reviewer sent two changes back for lacking a test with a multi-byte character.",
      advice: "Add a test with emoji before you change the tokenizer.",
      cards: [48, 50],
      retro_date: "2026-10-28",
      status: "active",
      retired_reason: null,
      retrievals: 4,
    },
    {
      id: 1,
      repository_id: 1,
      kind: "failure",
      situation: "Running the whole test suite",
      lesson: "The run failed after the shell timed out on the full suite.",
      advice: "Run only the tests of the crate you changed.",
      cards: [41],
      retro_date: null,
      status: "retired",
      retired_reason: "not retrieved for 60 days",
      retrievals: 0,
    },
  ];

  // The research.
  let nextResearch = 3;
  const research: Array<Json & { id: number; status: string }> = [
    {
      id: 2,
      repository_id: 1,
      goal: "Make the host build faster",
      measure: "cargo build --release 2>&1 | tail -1",
      better: "lower",
      budget: { kind: "tokens", amount: 200_000 },
      used: { experiments: 0, tokens: 0 },
      limit_reached: null,
      status: "draft",
      baseline: null,
      reason: "",
      tokens: 0,
      at: NOW(),
      experiments: [],
    },
    {
      id: 1,
      repository_id: 1,
      goal: "Cut the clean build time of the host",
      measure: "./bench/build.sh",
      better: "lower",
      budget: { kind: "tokens", amount: 500_000 },
      used: { experiments: 2, tokens: 211_400 },
      limit_reached: null,
      status: "done",
      baseline: 372,
      reason: "a round proposed no more levers",
      tokens: 211_400,
      at: NOW(),
      experiments: [
        {
          id: 1,
          research_id: 1,
          branch: "research/1-split-crates",
          title: "Split the host crate in two",
          value: 301,
          checks: true,
          kept: true,
          reason: "372 to 301 (-19.1 %), better than the baseline",
          change_id: 4,
        },
        {
          id: 2,
          research_id: 1,
          branch: "research/1-more-jobs",
          title: "Build with more jobs",
          value: 375,
          checks: null,
          kept: false,
          reason: "375 against the baseline 372: not better by more than 1 %",
          change_id: null,
        },
        {
          id: 3,
          research_id: 1,
          branch: "research/1-lld",
          title: "Link with lld",
          value: null,
          checks: null,
          kept: false,
          reason: "the measurement failed: the measure ended with exit status 127: lld: not found",
          change_id: null,
        },
      ],
    },
  ];
  const checkedBrief = (body: Json): string | null => {
    if (typeof body.goal !== "string" || body.goal.trim() === "") return "the goal is blank";
    if (typeof body.measure !== "string" || body.measure.trim() === "")
      return "the measure is blank";
    const budget = body.budget as { kind?: unknown; amount?: unknown } | undefined;
    if (!budget || (budget.kind !== "experiments" && budget.kind !== "tokens")) {
      return "the budget is experiments or tokens";
    }
    if (
      typeof budget.amount !== "number" ||
      !Number.isInteger(budget.amount) ||
      budget.amount <= 0
    ) {
      return `the budget in ${budget.kind} must be more than 0`;
    }
    return null;
  };

  // The decision model.
  const count = (
    kind: string,
    n: number,
    withOutcome: number,
    byPerson: number,
    heldOut: number,
  ) => ({
    kind,
    count: n,
    with_outcome: withOutcome,
    by_person: byPerson,
    held_out: heldOut,
  });
  const kinds = [
    count("approve_issue", 14, 12, 14, 1),
    count("merge", 9, 9, 9, 2),
    count("tool_call", 1204, 311, 0, 118),
  ];
  const exports: Array<{ id: number; at: number; kinds: unknown[] }> = [
    { id: 1, at: 1_793_000_000, kinds: [count("merge", 4, 4, 4, 0)] },
  ];
  const takeovers = [
    { id: 2, kind: "merge", agreement: 0.9, count: 80, status: "dismissed", at: NOW() },
    { id: 3, kind: "tool_call", agreement: 0.97, count: 100, status: "merged", at: NOW() },
  ];
  const decisionModel = () => ({
    configured: true,
    model: "moebius-decider",
    agreement: 0.95,
    min_judgements: 100,
    kinds: [
      { kind: "approve_issue", shadows: 0, agreement: null, failed: 0, dropped: 0, routed: false },
      { kind: "merge", shadows: 40, agreement: 0.85, failed: 3, dropped: 2, routed: false },
      {
        kind: "tool_call",
        shadows: 118,
        agreement: 0.975,
        failed: 0,
        dropped: 0,
        routed: takeovers.some((t) => t.kind === "tool_call" && t.status === "merged"),
      },
    ],
    takeovers,
  });

  async function handle(request: IncomingMessage, response: ServerResponse, path: string) {
    const method = request.method ?? "GET";
    const rules = path === "/v1/rules" || path === "/v1/repositories/1/settings/rules.md";
    const script = /^\/v1\/repositories\/1\/scripts\/(.+)$/.exec(path);
    const one =
      /^\/v1\/(automations|lessons|research|takeovers(?=\/\d+\/release$))\/(\d+)(?:\/(\w+))?$/.exec(
        path,
      );
    const oneExport = /^\/v1\/exports\/(\d+)$/.exec(path);
    const retroDay = /^\/v1\/retros\/(\d{4}-\d{2}-\d{2})$/.exec(path);
    const retroRetry = /^\/v1\/retros\/(\d{4}-\d{2}-\d{2})\/retry$/.exec(path);
    const known =
      rules ||
      script ||
      one ||
      oneExport ||
      retroDay ||
      retroRetry ||
      [
        "/v1/automations",
        "/v1/retros",
        "/v1/proposals",
        "/v1/lessons",
        "/v1/research",
        "/v1/decision-model",
        "/v1/judgements/kinds",
        "/v1/exports",
      ].includes(path);
    if (!known) return false;

    const raw = method === "GET" ? "" : await readBody(request);
    const body = (raw === "" ? {} : JSON.parse(raw)) as Json;
    if (method !== "GET") requests.push({ method, path, body });
    // A segment a person wrote reaches the server encoded ("scripts/Not%20started"); a test names it as it reads it.
    const plain = `${method} ${decoded(path)}`;
    const refusal = refusals.get(`${method} ${path}`) ?? refusals.get(plain);
    if (refusal) {
      refusals.delete(`${method} ${path}`);
      refusals.delete(plain);
      return reply(response, refusal.status, refusal.message);
    }

    if (path === "/v1/rules" && method === "GET") return reply(response, 200, rulesView());
    if (rules && method === "PUT") {
      const text = String(body.text ?? "");
      const parsed = parseRules(text);
      if (typeof parsed === "string") {
        return reply(response, 422, path === "/v1/rules" ? parsed : `rules.md: ${parsed}`);
      }
      const final = text === "" || text.endsWith("\n") ? text : `${text}\n`;
      if (path === "/v1/rules") {
        const changed = final !== proseText;
        proseText = final;
        return reply(response, 200, { ...rulesView(), ...(changed ? { commit: "a1b2c3d" } : {}) });
      }
      repositoryText = text.trimEnd();
      return reply(response, 200, { file: "rules.md", exists: true, text });
    }

    if (script) {
      const column = decodeURIComponent(script[1] as string);
      const file = SCRIPT_FILES[column.toLowerCase()];
      if (!file) return reply(response, 404, "no such column");
      const view = () => {
        const own = scripts.get(column.toLowerCase());
        return {
          column,
          path: `repos/demo/${file}.luau`,
          source: own === undefined ? "built-in" : "repository",
          text: own ?? `-- The ${column} column\nmoebius.wait("nothing to do in ${column}")\n`,
        };
      };
      if (method === "GET") return reply(response, 200, view());
      if (method === "PUT") {
        const text = String(body.text ?? "");
        const bad = text.split("\n").findIndex((line) => line.includes("!!"));
        if (bad >= 0) {
          return reply(
            response,
            422,
            `syntax error: [string "${file}"]:${bad + 1}: Unexpected '!'`,
          );
        }
        // A crude check: a '(' that is never closed is a syntax error at the end of the script.
        let open: number[] = [];
        for (const [index, line] of text.split("\n").entries()) {
          for (const char of line) {
            if (char === "(") open.push(index + 1);
            if (char === ")") open = open.slice(0, -1);
          }
        }
        if (open.length > 0) {
          return reply(
            response,
            422,
            `syntax error: [string "${file}"]:${text.split("\n").length + 1}: Expected ')' (to close '(' at line ${open.at(-1)}), got <eof>`,
          );
        }
        scripts.set(column.toLowerCase(), text.endsWith("\n") ? text : `${text}\n`);
        broadcast({ kind: "script_changed", data: { repository: 1, column } });
        return reply(response, 200, view());
      }
      if (method === "DELETE") {
        if (!scripts.delete(column.toLowerCase())) {
          return reply(response, 404, `a script of its own for the ${column} column in demo`);
        }
        broadcast({ kind: "script_changed", data: { repository: 1, column } });
        return reply(response, 200, view());
      }
    }

    if (path === "/v1/automations" && method === "GET") return reply(response, 200, automations);
    if (path === "/v1/automations" && method === "POST") {
      const problem = checkedAutomation(body);
      if (problem) return reply(response, 422, problem);
      const made = {
        id: nextAutomation++,
        name: String(body.name).trim(),
        schedule: String(body.schedule).trim(),
        agent: body.agent,
        prompt: String(body.prompt).trim(),
        repository_id: body.repository ?? null,
        harness_repository: body.harness === true,
        enabled: body.enabled !== false,
        next_firing: body.enabled === false ? null : IN(8),
        last_firing: null,
      };
      automations.push(made);
      return reply(response, 200, made);
    }
    if (path === "/v1/retros" && method === "GET") return reply(response, 200, retroDays);
    if (path === "/v1/proposals" && method === "GET") return reply(response, 200, proposals);
    if (retroDay && method === "GET") {
      const date = retroDay[1] as string;
      const day = retroDays.find((each) => each.date === date);
      if (!day) return reply(response, 404, "no such Retro");
      const full = date === "2026-10-28";
      return reply(response, 200, {
        date,
        status: day.status,
        warnings: full
          ? [
              "proposal/2026-10-28-code is left out: it changes src/main.rs, which is not harness configuration",
            ]
          : day.status === "failed"
            ? ["the server stopped while the analyst was going"]
            : [],
        report: { date, errors: full ? report.errors : [] },
        proposals: proposals.filter((each) => each.retro_date === date),
      });
    }
    if (retroRetry && method === "POST") {
      const date = retroRetry[1] as string;
      const day = retroDays.find((each) => each.date === date);
      if (!day) return reply(response, 404, `no Retro of ${date}`);
      if (day.status !== "failed") {
        return reply(response, 422, `the Retro of ${date} is ${day.status}, not failed`);
      }
      day.status = "proposed";
      broadcast({ kind: "retro_done", data: { date, status: "proposed", proposals: 0 } });
      return reply(response, 200, {
        date,
        status: "analysing",
        warnings: [],
        // The page reads the day again rather than showing this, so the report is the empty one.
        report: { date, errors: [] },
        proposals: proposals.filter((each) => each.retro_date === date),
      });
    }
    if (path === "/v1/retros" && method === "POST") {
      const date = typeof body.date === "string" ? body.date : today;
      if (retroDays.some((day) => day.date === date)) {
        return reply(response, 409, `the Retro of ${date} exists already`);
      }
      retroDays.unshift({ date, status: "reported", errors: 0, proposals: 0 });
      broadcast({ kind: "retro_done", data: { date, status: "reported" } });
      return reply(response, 200, {
        date,
        status: "reported",
        warnings: [],
        report: { date, errors: [] },
        proposals: [],
      });
    }
    if (path === "/v1/lessons" && method === "GET") return reply(response, 200, lessons);
    if (path === "/v1/research" && method === "GET") return reply(response, 200, research);
    if (path === "/v1/research" && method === "POST") {
      const problem = checkedBrief(body);
      if (problem) return reply(response, 422, problem);
      const made = {
        id: nextResearch++,
        repository_id: body.repository,
        goal: String(body.goal).trim(),
        measure: String(body.measure).trim(),
        better: body.better === "higher" ? "higher" : "lower",
        budget: body.budget,
        used: { experiments: 0, tokens: 0 },
        limit_reached: null,
        status: "draft",
        baseline: null,
        reason: "",
        tokens: 0,
        at: NOW(),
        experiments: [],
      };
      research.unshift(made);
      broadcast({ kind: "research_status", data: { research: made.id, status: "draft" } });
      return reply(response, 200, made);
    }
    if (path === "/v1/decision-model" && method === "GET")
      return reply(response, 200, decisionModel());
    if (path === "/v1/judgements/kinds" && method === "GET") return reply(response, 200, kinds);
    if (path === "/v1/exports" && method === "GET")
      return reply(response, 200, [...exports].reverse());
    if (path === "/v1/exports" && method === "POST") {
      const made = { id: exports.length + 1, at: NOW(), kinds };
      exports.push(made);
      return reply(response, 200, made);
    }

    if (oneExport && method === "DELETE") {
      const at = exports.findIndex((each) => each.id === Number(oneExport[1]));
      if (at < 0) return reply(response, 404, "no such export");
      const [gone] = exports.splice(at, 1);
      broadcast({ kind: "export_deleted", data: { export: Number(oneExport[1]) } });
      return reply(response, 200, gone);
    }

    if (one) {
      const [, collection, idText, action] = one;
      const id = Number(idText);
      if (collection === "automations") {
        const automation = automations.find((each) => each.id === id);
        if (!automation) return reply(response, 404, "no such automation");
        if (!action && method === "GET") return reply(response, 200, automation);
        if (!action && method === "PUT") {
          const problem = checkedAutomation(body);
          if (problem) return reply(response, 422, problem);
          Object.assign(automation, {
            name: String(body.name).trim(),
            schedule: String(body.schedule).trim(),
            agent: body.agent,
            prompt: String(body.prompt).trim(),
            repository_id: body.repository ?? null,
            harness_repository: body.harness === true,
            enabled: body.enabled !== false,
            next_firing: body.enabled === false ? null : IN(8),
          });
          return reply(response, 200, automation);
        }
        if (!action && method === "DELETE") {
          automations.splice(automations.indexOf(automation), 1);
          return reply(response, 200, {});
        }
        if (action === "firings" && method === "GET") {
          return reply(response, 200, firings.get(id) ?? []);
        }
        if (action === "run" && method === "POST") {
          const firing: Firing = {
            id: nextFiring++,
            status: "started",
            run_id: 52,
            chat_status: "running",
            manual: true,
            reason: "",
            at: NOW(),
          };
          firings.set(id, [firing, ...(firings.get(id) ?? [])]);
          automation.last_firing = firing;
          broadcast({ kind: "automation_fired", data: { automation: id, status: "started" } });
          return reply(response, 200, firing);
        }
      }
      if (collection === "lessons") {
        const lesson = lessons.find((each) => each.id === id);
        if (!lesson) return reply(response, 404, "no such lesson");
        if (method === "PUT") {
          for (const part of ["situation", "lesson", "advice"]) {
            if (typeof body[part] !== "string" || (body[part] as string).trim() === "") {
              return reply(response, 422, `the ${part} is empty`);
            }
          }
          Object.assign(lesson, {
            situation: body.situation,
            lesson: body.lesson,
            advice: body.advice,
          });
          return reply(response, 200, lesson);
        }
        if (method === "DELETE") {
          lessons = lessons.filter((each) => each.id !== id);
          return reply(response, 200, {});
        }
      }
      if (collection === "research") {
        const item = research.find((each) => each.id === id);
        if (!item) return reply(response, 404, "no such research");
        if (!action && method === "DELETE") {
          // A research that runs is stopped first, as the server does.
          if (item.status === "running") {
            item.status = "stopped";
            broadcast({ kind: "research_status", data: { research: id, status: "stopped" } });
          }
          research.splice(research.indexOf(item), 1);
          broadcast({ kind: "research_deleted", data: { research: id } });
          return reply(response, 200, item);
        }
        if (!action && method === "PUT") {
          if (item.status !== "draft") return reply(response, 409, "only a draft can be edited");
          const problem = checkedBrief(body);
          if (problem) return reply(response, 422, problem);
          Object.assign(item, {
            goal: String(body.goal).trim(),
            measure: String(body.measure).trim(),
            better: body.better,
            budget: body.budget,
          });
          broadcast({ kind: "research_status", data: { research: id, status: item.status } });
          return reply(response, 200, item);
        }
        const moves: Record<string, Record<string, string>> = {
          confirm: { draft: "confirmed" },
          start: { confirmed: "running" },
          stop: { draft: "stopped", confirmed: "stopped", running: "stopped" },
        };
        if (action && method === "POST" && moves[action]) {
          const to = moves[action]?.[item.status];
          if (!to) return reply(response, 409, `the research is ${item.status}`);
          item.status = to;
          broadcast({ kind: "research_status", data: { research: id, status: to } });
          return reply(response, 200, item);
        }
      }
      if (collection === "takeovers" && action === "release" && method === "POST") {
        const takeover = takeovers.find((each) => each.id === id);
        if (!takeover) return reply(response, 404, "no such takeover");
        if (takeover.status !== "merged") return reply(response, 409, "the takeover is not merged");
        takeover.status = "released";
        broadcast({ kind: "takeover_status", data: { takeover: id, status: "released" } });
        return reply(response, 200, takeover);
      }
    }
    return reply(response, 404, "no such route");
  }

  return {
    handle,
    requests,
    scripts,
    research,
    days: retroDays,
    refuseNext(method, path, status, message) {
      refusals.set(`${method} ${path}`, { status, message });
    },
  };
}
