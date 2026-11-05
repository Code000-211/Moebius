/**
 * The routes of the Inbox, Projects, Fleet, Usage, Get started and Settings pages in the fake server, with the JSON the
 * real server sends (docs/reference/desktop-api.md). It sits in front of the board's fake: it answers the snapshot
 * itself, adding to the board's what the pages need (more repositories, the repository's edited trusted logins, the
 * inbox items of proposals, takeovers and grillings, a change of the harness repository), and the routes of the
 * repositories' files and plugin settings (one sandbox plugin, with a repository's and the server's settings), the usage, the settings with their restart, the
 * webhooks, the first-start steps and the decisions of the inbox. The state changes the way the real server's does, and the events it sends are the real ones.
 * A test can make the next request to a route fail with the server's own sentence (`refuseNext`).
 */
import type { IncomingMessage, ServerResponse } from "node:http";
import type { BoardFake } from "./board";

type Send = (event: { kind: string; data: unknown }) => void;

interface Repository {
  id: number;
  name: string;
  path: string;
  forge: string;
  default_branch: string;
  github: string | null;
  trusted: string[];
  project: string | null;
}

interface InboxItem {
  id: number;
  kind: string;
  card_id: number | null;
  title: string;
  data: Record<string, unknown>;
  resolved_at: number | null;
  at: number;
}

interface Setting {
  value: unknown;
  default: boolean;
}

export interface PagesAFake {
  /** Answers a route of these pages and returns true, or returns false for any other path. */
  handle(request: IncomingMessage, response: ServerResponse, path: string): Promise<boolean>;
  /** The non-GET requests received, in order. */
  readonly requests: Array<{ method: string; path: string; body: unknown }>;
  /** Makes the next request with `method` to `path` (with its query) fail with `status` and `message`. */
  refuseNext(method: string, path: string, status: number, message: string): void;
  /** Puts an item in the inbox, as the board engine or a part of the server does. */
  addInboxItem(item: Partial<InboxItem> & { kind: string; title: string }): InboxItem;
  /** Makes every step of the first start done. */
  completeStart(): void;
  /** Makes a step of the first start (by its key) to do, with what the server says of it. */
  missStep(key: string, detail: string): void;
}

const NOW = 1_790_000_000;

/** The date of the server in the usage report. */
export const TODAY = "2026-10-28";

const FILES: Record<string, string> = {
  "README.md": "# demo\n\nA demo repository.\n",
  "docs/guide.md": "# Guide\n\nRead the code.\n",
  "src/main.rs": '//! The demo server.\n\nfn main() {\n    println!("hello");\n}\n',
  "src/server/main.rs": "use a;\nuse b;\nmod main_loop;\n",
  "src/remain.rs": "// What remains.\n",
};

const BINARY = { path: "logo.png", size: 48_211, binary: true };
const LARGE = { path: "data.bin", size: 2_000_000, too_large: true };

/** The keys of the Settings page with a value, as `GET /v1/settings` has them. */
function initialSettings(): Record<string, Setting> {
  const set = (value: unknown, isDefault: boolean): Setting => ({ value, default: isDefault });
  return {
    data_dir: set("/srv/moebius/.moebius", true),
    listen: set("127.0.0.1:7171", true),
    token_file: set("/srv/moebius/.moebius/token", true),
    "router.listen": set("127.0.0.1:7172", true),
    run_limit: set(3, true),
    "board.tick_ms": set(2000, true),
    "board.round_limit": set(7, false),
    "budgets.day_tokens": set(8_000_000, false),
    "checks.timeout_seconds": set(600, true),
    retro_at: set("03:00", true),
    "trial_window.start": set("00:00", true),
    "trial_window.end": set("06:00", true),
    "regression.size": set(5, true),
    "regression.gate": set(true, true),
    "harness.url": set(null, true),
    "harness.mcp_env": set([], true),
    rules_file: set(null, true),
    "pi_bolt.program": set("pi-bolt", true),
    "github.app_id": set(null, true),
    "github.private_key_file": set(null, true),
    "github.api_url": set(null, true),
    "github.poll_seconds": set(null, true),
    "gitlab.url": set(null, true),
    "gitlab.token_file": set(null, true),
    "gitlab.token_secret": set(null, true),
    "gitlab.poll_seconds": set(null, true),
    "forgejo.url": set(null, true),
    "forgejo.token_file": set(null, true),
    "forgejo.token_secret": set(null, true),
    "forgejo.poll_seconds": set(null, true),
  };
}

/** The defaults a key goes back to when it is removed from the file. */
const DEFAULTS: Record<string, unknown> = {
  "board.round_limit": 5,
  "budgets.day_tokens": null,
  retro_at: "03:00",
  "github.app_id": null,
  "github.private_key_file": null,
};

/** The keys whose new value the running server uses only after it restarts. */
const RESTART_KEYS = new Set(["listen", "router.listen", "data_dir", "token_file", "run_limit"]);

function readBody(request: IncomingMessage): Promise<string> {
  return new Promise((done) => {
    const chunks: Buffer[] = [];
    request.on("data", (chunk: Buffer) => chunks.push(chunk));
    request.on("end", () => done(Buffer.concat(chunks).toString("utf8")));
  });
}

function usageReport(days: number) {
  const scale = days;
  return {
    today: TODAY,
    days_in_window: days,
    totals: {
      tokens: 1_840_000 * scale,
      calls: 1284 * scale,
      cached_share: 0.61,
      model_ms: 11_520_000 * scale,
      day_budget: 8_000_000,
    },
    days: [
      { date: "2026-10-24", tokens: 2_520_000, model: "claude-sonnet" },
      { date: "2026-10-26", tokens: 900_000, model: "qwen-flash" },
      { date: "2026-10-27", tokens: 1_200_000, model: "claude-sonnet" },
      { date: TODAY, tokens: 1_840_000, model: "qwen-flash" },
    ],
    models: [
      {
        model: "qwen-flash",
        tokens: 960_000 * scale,
        calls: 690 * scale,
        input_tokens: 700_000 * scale,
        cached_tokens: 520_000 * scale,
      },
      {
        model: "claude-sonnet",
        tokens: 880_000 * scale,
        calls: 594 * scale,
        input_tokens: 600_000 * scale,
        cached_tokens: 300_000 * scale,
      },
    ],
    agents: [
      {
        agent: "Coder",
        calls: 702 * scale,
        tokens: 1_120_000 * scale,
        input_tokens: 1_000_000 * scale,
        cached_tokens: 610_000 * scale,
        model_ms: 7_080_000 * scale,
      },
      {
        agent: "Reviewer",
        calls: 300 * scale,
        tokens: 400_000 * scale,
        input_tokens: 350_000 * scale,
        cached_tokens: 100_000 * scale,
        model_ms: 2_000_000 * scale,
      },
    ],
    activities: [
      { activity: "bash", model_ms: 2_140_000 * scale, calls: 214 * scale, output_tokens: 61_000 },
      {
        activity: "answering",
        model_ms: 1_000_000 * scale,
        calls: 40 * scale,
        output_tokens: 9000,
      },
    ],
    tool_waits: [{ tool: "bash", calls: 31, seconds: 2460 }],
    classes: [],
    tool_output: { results: 96, bytes_before: 3_400_000, bytes_after: 920_000 },
    lens: [
      { phase: "deep", tool: "cargo", runs: 31, skipped: 2, ms: 412_000, shown: 9 },
      { phase: "lint", tool: "ruff", runs: 140, skipped: 0, ms: 5200, shown: 17 },
    ],
  };
}

export function createPagesAFake(send: Send, board: BoardFake): PagesAFake {
  const requests: PagesAFake["requests"] = [];
  const refusals = new Map<string, { status: number; message: string }>();
  let clock = NOW;

  let repositories: Repository[] = [];
  const removed = new Set<number>();
  const trusted = new Map<number, string[]>();
  /** The values of the sandbox plugin's app settings. */
  let appValues: Record<string, string> = {};
  const appSettings = () => ({
    plugin: "podman-sandbox",
    version: "0.2.0",
    settings: [
      {
        key: "registry",
        label: "Registry",
        description: "Where the base images of the Dockerfiles come from.",
        input: "line",
        value: appValues.registry ?? "",
      },
    ],
  });
  /** The values of the sandbox plugin's project settings, by repository. */
  const pluginValues = new Map<number, Record<string, string>>();
  const pluginSettings = (id: number) => {
    const values = pluginValues.get(id) ?? {};
    return {
      plugin: "podman-sandbox",
      version: "0.2.0",
      settings: [
        {
          key: "dockerfile",
          label: "Dockerfile",
          description: "The image of this repository's runs, built by the plugin before a run.",
          input: "code",
          language: "dockerfile",
          value: values.dockerfile ?? "",
        },
        {
          key: "network",
          label: "Network",
          description: "moebius or internet.",
          input: "line",
          value: values.network ?? "",
        },
      ],
    };
  };
  let nextRepository = 2;

  const inbox: InboxItem[] = [];
  let nextItem = 100;

  const settings = initialSettings();
  const restartKeys = new Set<string>();
  const webhooks = [
    {
      forge: "github",
      path: "/v1/webhooks/github",
      header: "X-Hub-Signature-256",
      secret: "whsec_0123456789abcdef0123456789abcdef" as string | null,
    },
    {
      forge: "gitlab",
      path: "/v1/webhooks/gitlab",
      header: "X-Gitlab-Token",
      secret: null as string | null,
    },
  ];
  let secrets = 0;

  const steps = [
    {
      key: "upstream",
      title: "Connect a model provider",
      done: true,
      detail: "3 models are available.",
    },
    {
      key: "pi_bolt",
      title: "Install Pi-Bolt",
      done: false,
      detail: "Could not run pi-bolt: it was not found. Set the program on the Settings page.",
    },
    {
      key: "harness",
      title: "Keep agents, skills and MCP servers in a repository",
      done: false,
      detail: "Agents and rules are in moebius.toml, and skills and MCP servers need a repository.",
    },
    {
      key: "coder",
      title: "Choose the Coder's model",
      done: true,
      detail: "The Coder uses fake-coder.",
    },
    { key: "repository", title: "Add a repository", done: true, detail: "1 repository is added." },
  ];

  const fail = (response: ServerResponse, status: number, message: string): true => {
    response.writeHead(status, { "content-type": "text/plain" });
    response.end(message);
    return true;
  };
  const ok = (response: ServerResponse, body: unknown, status = 200): true => {
    response.writeHead(status, { "content-type": "application/json" });
    response.end(JSON.stringify(body));
    return true;
  };

  function seedInbox() {
    const item = (fields: Partial<InboxItem> & { kind: string; title: string }) => add(fields);
    item({
      kind: "proposal",
      title: "Let the Coder read the tests first",
      data: {
        proposal: 4,
        branch: "proposal/2026-10-25-read-tests",
        trials: [
          { card: 3, outcome: "positive", reason: "It passed in one round." },
          { card: null, outcome: "untried", reason: "No card fits." },
        ],
        regression: [{ card: 2, outcome: "negative", reason: "It broke the build." }],
      },
      at: NOW - 400,
    });
    item({
      kind: "proposal",
      title: "Raise the round limit to 7",
      data: {
        proposal: 5,
        kind: "settings",
        repository: null,
        reason: "Cards ran out of rounds before the review passed.",
        diff: "--- a/moebius.toml\n+++ b/moebius.toml\n@@ -1 +1 @@\n-round_limit = 5\n+round_limit = 7\n",
      },
      at: NOW - 500,
    });
    item({
      kind: "takeover",
      title: "The decision model can take over tool_call",
      data: { takeover: 2, kind: "tool_call", agreement: 0.95, count: 120 },
      at: NOW - 600,
    });
    item({
      kind: "grilling_issues",
      title: "3 issues from a grilling",
      data: {
        grilling: 7,
        repository: 1,
        issues: [
          { title: "Add the export button", body: "On the report page.", blocked_by: [] },
          { title: "Write the CSV", body: "Its columns.", blocked_by: [0] },
          { title: "Document the export", body: "In the guide.", blocked_by: [0, 1] },
        ],
      },
      at: NOW - 700,
    });
    item({
      kind: "grilling_report",
      title: "A report from a grilling",
      data: { grilling: 8, repository: 1, report: "The export is slow because of the join." },
      at: NOW - 800,
    });
    item({
      kind: "ready_to_merge",
      title: "Add a retry helper to the downloader",
      card_id: null,
      data: { change: 9, branch: "moebius/chat-9" },
      at: NOW - 900,
    });
  }

  function add(fields: Partial<InboxItem> & { kind: string; title: string }): InboxItem {
    const made: InboxItem = {
      id: nextItem++,
      card_id: null,
      data: {},
      resolved_at: null,
      at: ++clock,
      ...fields,
    };
    inbox.push(made);
    return made;
  }

  // The two proposals the inbox holds items for, and the Retro day that made them, so the card can show what a bare
  // item does not carry and link to the day that explains it.
  const retroDay = "2026-10-25";
  const retros = [{ date: retroDay, status: "proposed", errors: 1, proposals: 2 }];
  const proposals = [
    {
      id: 4,
      kind: "harness",
      retro_date: retroDay,
      branch: "proposal/2026-10-25-read-tests",
      title: "Let the Coder read the tests first",
      body: "Two of the Coder's changes failed a test that was already in the repository; its instructions now say to read the tests a change has to pass before writing it.",
      agents: ["Coder"],
      cards: [3],
      status: "positive",
      trials: [],
      diff: "",
    },
    {
      id: 5,
      kind: "settings",
      retro_date: retroDay,
      branch: "proposal/2026-10-25-round-limit",
      title: "Raise the round limit to 7",
      body: "Cards ran out of rounds before the review passed.",
      agents: [],
      cards: [],
      status: "untried",
      trials: [],
      diff: "--- a/moebius.toml\n+++ b/moebius.toml\n@@ -1 +1 @@\n-round_limit = 5\n+round_limit = 7\n",
    },
  ];

  const resolveItem = (kind: string, key: string, id: number) => {
    for (const item of inbox) {
      if (item.kind === kind && item.data[key] === id && item.resolved_at === null) {
        item.resolved_at = ++clock;
      }
    }
  };

  const repositoryView = (repo: Repository): Repository => ({
    ...repo,
    trusted: trusted.get(repo.id) ?? repo.trusted,
  });

  function snapshot() {
    const base = board.snapshot() as {
      boards: Array<{ repository: Repository }>;
      harness_changes: unknown[];
      inbox: unknown[];
    };
    return {
      boards: [
        ...base.boards
          .filter((each) => !removed.has(each.repository.id))
          .map((each) => ({ ...each, repository: repositoryView(each.repository) })),
        ...repositories
          .filter((each) => !removed.has(each.id))
          .map((repository) => ({
            repository: repositoryView(repository),
            cards: [],
            round_limit: 5,
            waiting: {},
            changes: [],
          })),
      ],
      harness_changes: [
        {
          id: 9,
          repository_id: null,
          harness_repository: true,
          card_id: null,
          branch: "moebius/chat-9",
          state: "open",
          pull_request: null,
          checks: "pending",
          review: null,
          head: null,
          published_at: null,
        },
      ],
      inbox: [...base.inbox, ...inbox.filter((item) => item.resolved_at === null)],
    };
  }

  function settingsView() {
    return {
      file: "/srv/moebius/moebius.toml",
      values: settings,
      secrets: ["anthropic"],
      restart_needed: restartKeys.size > 0,
      restart_keys: [...restartKeys],
    };
  }

  function applySettings(body: { set?: Record<string, unknown>; remove?: string[] }) {
    for (const key of body.remove ?? []) {
      // A table (`github`) takes its keys with it.
      for (const known of Object.keys(settings)) {
        if (known === key || known.startsWith(`${key}.`)) {
          settings[known] = { value: DEFAULTS[known] ?? null, default: true };
        }
      }
    }
    for (const [key, value] of Object.entries(body.set ?? {})) {
      settings[key] = { value, default: false };
      if (RESTART_KEYS.has(key)) restartKeys.add(key);
    }
  }

  function validateSettings(body: {
    set?: Record<string, unknown>;
    remove?: string[];
  }): string | null {
    for (const key of Object.keys(body.set ?? {})) {
      if (!(key in settings)) return `unknown key: ${key}`;
    }
    const runLimit = body.set?.run_limit;
    if (typeof runLimit === "number" && runLimit < 1) return "run_limit must be at least 1";
    return null;
  }

  type Found = {
    files: Array<{ path: string; hits: Array<{ line: number; text: string }> }>;
    total: number;
    truncated: boolean;
  };

  function search(query: string, regex: boolean): Found | { error: string } {
    if (query.trim() === "") return { error: "the query is blank" };
    if (regex && query === "(") return { error: "fatal: unmatched parenthesis" };
    let test: (line: string) => boolean;
    if (regex) {
      const pattern = new RegExp(query, "i");
      test = (line) => pattern.test(line);
    } else {
      test = (line) => line.toLowerCase().includes(query.toLowerCase());
    }
    const files = Object.entries(FILES)
      .map(([path, text]) => ({
        path,
        hits: text
          .split("\n")
          .map((line, at) => ({ line: at + 1, text: line }))
          .filter((hit) => test(hit.text)),
      }))
      .filter((file) => file.hits.length > 0)
      .sort((a, b) => a.path.localeCompare(b.path));
    return {
      files,
      total: files.reduce((sum, file) => sum + file.hits.length, 0),
      truncated: false,
    };
  }

  /** The settings of each repository's project: the files it has, by name. The first one has a review note. */
  const projectFiles = new Map<number, Map<string, string>>([
    [1, new Map([["review.md", "Prefer small functions.\n"]])],
  ]);
  const SETTING_NAMES = ["rules.md", "review.md", "lint.toml", "checks", "test-first", "localize"];
  const projectFilesOf = (id: number) => {
    const found = projectFiles.get(id) ?? new Map<string, string>();
    projectFiles.set(id, found);
    return found;
  };
  const projectSettingsView = (own: Map<string, string>, id: number) => ({
    path: `repos/${id === 1 ? "demo" : `repository-${id}`}`,
    editable: true,
    files: SETTING_NAMES.map((file) => ({
      file,
      exists: own.has(file),
      text: own.get(file) ?? "",
    })),
    scripts: [],
    leftover: id === 1 ? [".moebius/old.luau"] : [],
  });

  function tree(directory: string) {
    const names = new Map<string, "dir" | "file">();
    const all = [...Object.keys(FILES), "logo.png", "data.bin"];
    for (const path of all) {
      if (directory !== "" && !path.startsWith(`${directory}/`)) continue;
      const rest = directory === "" ? path : path.slice(directory.length + 1);
      const [head, ...tail] = rest.split("/");
      if (head) names.set(head, tail.length > 0 ? "dir" : "file");
    }
    const entries = [...names].map(([name, kind]) => ({
      name,
      path: directory === "" ? name : `${directory}/${name}`,
      kind,
      size:
        kind === "dir"
          ? 0
          : (FILES[directory === "" ? name : `${directory}/${name}`]?.length ?? 1000),
    }));
    entries.sort(
      (a, b) => (a.kind === b.kind ? 0 : a.kind === "dir" ? -1 : 1) || a.name.localeCompare(b.name),
    );
    return { path: directory, entries };
  }

  const repositoryRequest = (
    body: Record<string, unknown>,
  ): { repository: Repository } | { error: string } => {
    const name = typeof body.name === "string" ? body.name : "";
    const url = typeof body.url === "string" ? body.url : "";
    if (!/^[A-Za-z0-9_-][A-Za-z0-9._-]*$/.test(name)) {
      return { error: `invalid repository name: ${name}` };
    }
    const all = [
      ...(board.snapshot() as { boards: Array<{ repository: Repository }> }).boards.map(
        (b) => b.repository,
      ),
      ...repositories,
    ];
    if (all.some((each) => each.name === name && !removed.has(each.id))) {
      return { error: `a repository named ${name} exists already` };
    }
    const forge = typeof body.forge === "string" ? body.forge : "none";
    const github = typeof body.github === "string" ? body.github : null;
    const project = typeof body.project === "string" ? body.project : null;
    const list = Array.isArray(body.trusted) ? (body.trusted as string[]) : [];
    if (forge === "none" && (github || project || list.length > 0)) {
      return { error: "a repository without a forge has no github, project or trusted logins" };
    }
    if (forge === "github" && !/^[\w.-]+\/[\w.-]+$/.test(github ?? "")) {
      return { error: "github must be owner/repo" };
    }
    if (url.includes("unreachable")) return { error: "git clone failed: repository not found" };
    return {
      repository: {
        id: nextRepository++,
        name,
        path: `/srv/.moebius/repos/${name}`,
        forge,
        default_branch: "main",
        github,
        trusted: list,
        project,
      },
    };
  };

  const fake: PagesAFake = {
    requests,
    refuseNext(method, path, status, message) {
      refusals.set(`${method} ${path}`, { status, message });
    },
    addInboxItem: add,
    completeStart() {
      for (const step of steps) step.done = true;
    },
    missStep(key, detail) {
      const step = steps.find((each) => each.key === key);
      if (step) Object.assign(step, { done: false, detail });
    },

    async handle(request, response, path) {
      const method = request.method ?? "GET";
      const url = new URL(request.url ?? "/", "http://x");
      const mine =
        path === "/v1/snapshot" ||
        path === "/v1/start" ||
        path === "/v1/usage" ||
        path === "/v1/settings" ||
        path === "/v1/plugins/settings" ||
        path === "/v1/plugins/podman-sandbox/settings" ||
        path === "/v1/proposals" ||
        path === "/v1/server" ||
        path === "/v1/server/restart" ||
        path === "/v1/retros" ||
        /^\/v1\/retros\/\d{4}-\d{2}-\d{2}$/.test(path) ||
        path === "/v1/webhooks" ||
        path === "/v1/harness/setup" ||
        path === "/v1/repositories" ||
        /^\/v1\/webhooks\/[a-z]+\/secret$/.test(path) ||
        /^\/v1\/repositories\/\d+(\/(tree|files|search|settings|plugin-settings)(\/.*)?)?$/.test(path) ||
        /^\/v1\/(proposals|takeovers|grillings)\/\d+\/(merge|dismiss|confirm)$/.test(path) ||
        /^\/v1\/grillings\/\d+$/.test(path) ||
        path === "/v1/changes/9/merge" ||
        path === "/v1/runs/1";
      if (!mine) return false;
      const key = `${method} ${path}${url.search}`;
      const refusal = refusals.get(key) ?? refusals.get(`${method} ${path}`);
      if (refusal) {
        refusals.delete(key);
        refusals.delete(`${method} ${path}`);
        return fail(response, refusal.status, refusal.message);
      }
      const body: Record<string, unknown> =
        method === "GET" ? {} : JSON.parse((await readBody(request)) || "{}");
      if (method !== "GET") requests.push({ method, path, body });

      if (path === "/v1/snapshot") return ok(response, snapshot());
      if (path === "/v1/start") {
        return ok(response, {
          done: steps.every((step) => step.done),
          steps,
          warnings: [],
        });
      }
      if (path === "/v1/usage") {
        const days = Number(url.searchParams.get("days") ?? 1);
        if (!Number.isInteger(days) || days < 1 || days > 365) {
          return fail(response, 422, "days must be a whole number from 1 to 365");
        }
        return ok(response, usageReport(days));
      }
      if (path === "/v1/proposals" && method === "GET") return ok(response, proposals);
      if (path === "/v1/retros" && method === "GET") return ok(response, retros);
      if (/^\/v1\/retros\/\d{4}-\d{2}-\d{2}$/.test(path) && method === "GET") {
        const date = path.slice("/v1/retros/".length);
        if (date !== retroDay) return fail(response, 404, "no such Retro");
        return ok(response, {
          date,
          status: "proposed",
          warnings: [],
          report: { date, errors: [] },
          proposals,
        });
      }
      if (path === "/v1/settings" && method === "GET") return ok(response, settingsView());
      if (path === "/v1/plugins/settings") return ok(response, { plugins: [appSettings()] });
      if (path === "/v1/plugins/podman-sandbox/settings" && method === "PUT") {
        const values = (body.values ?? {}) as Record<string, string>;
        const unknown = Object.keys(values).find((key) => key !== "registry");
        if (unknown) return fail(response, 422, `podman-sandbox has no setting "${unknown}"`);
        appValues = values;
        return ok(response, appSettings());
      }
      if (path === "/v1/settings" && method === "PUT") {
        const problem = validateSettings(body);
        if (problem) return fail(response, 422, problem);
        applySettings(body);
        return ok(response, {
          restart_needed: restartKeys.size > 0,
          restart_keys: [...restartKeys],
        });
      }
      if (path === "/v1/server") {
        return ok(response, {
          program: "/srv/moebius/target/debug/moebius",
          version: "0.0.0-fake",
          config_file: "/srv/moebius/moebius.toml",
          data_dir: "/srv/moebius/.moebius",
          api_address: "http://127.0.0.1:7171",
          router_address: "http://127.0.0.1:7172",
          host: "fake-host",
          os: "linux",
          arch: "x86_64",
        });
      }
      if (path === "/v1/server/restart") {
        // The server comes back with what the file says: nothing is left to restart.
        restartKeys.clear();
        response.writeHead(202);
        response.end();
        return true;
      }
      if (path === "/v1/harness/setup") {
        const harness = steps.find((step) => step.key === "harness");
        if (harness) {
          harness.done = true;
          harness.detail = "Agents, skills, MCP servers and rules come from /srv/moebius/harness.";
        }
        settings["harness.url"] = { value: "/srv/moebius/harness", default: false };
        restartKeys.add("harness.url");
        return ok(response, { restart_needed: true, restart_keys: ["harness.url"] });
      }
      if (path === "/v1/webhooks") return ok(response, webhooks);
      const secret = /^\/v1\/webhooks\/([a-z]+)\/secret$/.exec(path);
      if (secret) {
        const hook = webhooks.find((each) => each.forge === secret[1]);
        if (!hook) return fail(response, 404, "no such forge");
        secrets += 1;
        hook.secret = `whsec_made${secrets}${"0".repeat(24)}`;
        return ok(response, hook);
      }

      if (path === "/v1/repositories" && method === "POST") {
        const made = repositoryRequest(body);
        if ("error" in made) {
          return fail(response, made.error.startsWith("git clone") ? 500 : 422, made.error);
        }
        repositories = [...repositories, made.repository];
        send({ kind: "repository_added", data: made.repository });
        return ok(response, made.repository);
      }
      const repository =
        /^\/v1\/repositories\/(\d+)(?:\/(tree|files|search|settings|plugin-settings)(\/.*)?)?$/.exec(path);
      if (repository) {
        const id = Number(repository[1]);
        const all = [
          ...(board.snapshot() as { boards: Array<{ repository: Repository }> }).boards.map(
            (b) => b.repository,
          ),
          ...repositories,
        ];
        const found = all.find((each) => each.id === id && !removed.has(id));
        if (!found) return fail(response, 404, "no such repository");
        const part = repository[2];
        if (!part && method === "PUT") {
          if (found.forge === "none") return fail(response, 422, "the repository is on no forge");
          const list = (Array.isArray(body.trusted) ? (body.trusted as string[]) : []).map((l) =>
            l.trim(),
          );
          if (list.some((login) => !/^[\w.[\]-]+$/.test(login))) {
            return fail(response, 422, "a login is blank or has other characters");
          }
          trusted.set(id, [...new Set(list)]);
          const view = repositoryView(found);
          send({ kind: "repository_updated", data: view });
          return ok(response, view);
        }
        if (!part && method === "DELETE") {
          if (id === 1) {
            return fail(response, 409, "run 1 of demo is still going: stop it first");
          }
          removed.add(id);
          const view = repositoryView(found);
          send({ kind: "repository_removed", data: view });
          return ok(response, view);
        }
        if (part === "settings") {
          const mine = projectFilesOf(id);
          if (!repository[3]) {
            if (method !== "GET") return false;
            return ok(response, projectSettingsView(mine, id));
          }
          const name = repository[3].slice(1);
          if (!SETTING_NAMES.includes(name)) {
            return fail(response, 422, `${name} is not a file of a project's settings`);
          }
          if (method === "PUT") {
            const text = String(body.text ?? "");
            if (name === "lint.toml" && /^\s*unknown\s*=/m.test(text)) {
              return fail(response, 422, "lint.toml: unknown field `unknown`");
            }
            mine.set(name, text);
            send({ kind: "project_settings_changed", data: { repository: id, file: name } });
            return ok(response, { file: name, exists: true, text, commit: "a1b2c3d" });
          }
          if (method === "DELETE") {
            if (!mine.delete(name)) return fail(response, 404, `${name} of the project`);
            send({ kind: "project_settings_changed", data: { repository: id, file: name } });
            return ok(response, { file: name, exists: false, text: "", commit: "a1b2c3d" });
          }
          return false;
        }
        if (part === "tree" && method === "GET") {
          const directory = url.searchParams.get("path") ?? "";
          const listing = tree(directory);
          if (directory !== "" && listing.entries.length === 0) {
            return fail(response, 404, "no such directory");
          }
          return ok(response, listing);
        }
        if (part === "files" && !repository[3] && method === "GET") {
          return ok(response, { paths: [...Object.keys(FILES), "logo.png", "data.bin"].sort() });
        }
        if (part === "files" && repository[3]) {
          const file = decodeURIComponent(repository[3].slice(1));
          if (method === "GET") {
            if (file === "logo.png") return ok(response, BINARY);
            if (file === "data.bin") return ok(response, LARGE);
            const text = FILES[file];
            if (text === undefined) return fail(response, 404, "no such file");
            return ok(response, { path: file, size: text.length, text });
          }
          if (method === "PUT") {
            if (typeof body.text !== "string") return fail(response, 422, "text is needed");
            FILES[file] = body.text;
            send({ kind: "file_changed", data: { repository: id, path: file } });
            return ok(response, { path: file, size: body.text.length, text: body.text });
          }
        }
        if (part === "plugin-settings" && !repository[3] && method === "GET") {
          return ok(response, { repository: id, plugins: [pluginSettings(id)] });
        }
        if (part === "plugin-settings" && repository[3] === "/podman-sandbox" && method === "PUT") {
          const values = (body.values ?? {}) as Record<string, string>;
          const known = ["dockerfile", "network"];
          const unknown = Object.keys(values).find((key) => !known.includes(key));
          if (unknown) return fail(response, 422, `podman-sandbox has no setting "${unknown}"`);
          if ((values.network ?? "").includes("\n")) {
            return fail(response, 422, "Network is one line");
          }
          pluginValues.set(id, values);
          return ok(response, pluginSettings(id));
        }
        if (part === "search" && method === "GET") {
          const result = search(
            url.searchParams.get("q") ?? "",
            url.searchParams.get("regex") === "true",
          );
          return "error" in result ? fail(response, 422, result.error) : ok(response, result);
        }
        return false;
      }

      const deleting = /^\/v1\/grillings\/(\d+)$/.exec(path);
      if (deleting && method === "DELETE") {
        // A grilling is deleted with its inbox item, whether it was answered or not.
        const id = Number(deleting[1]);
        const at = inbox.findIndex(
          (each) => each.kind.startsWith("grilling_") && each.data.grilling === id,
        );
        if (at < 0) return fail(response, 404, "grilling not found");
        inbox.splice(at, 1);
        send({ kind: "grilling_deleted", data: { grilling: id } });
        return ok(response, { id });
      }
      const decision =
        /^\/v1\/(proposals|takeovers|grillings)\/(\d+)\/(merge|dismiss|confirm)$/.exec(path);
      if (decision && method === "POST") {
        const [, kind, idText, action] = decision as unknown as [string, string, string, string];
        const id = Number(idText);
        const itemKind =
          kind === "proposals" ? "proposal" : kind === "takeovers" ? "takeover" : "grilling_";
        const key =
          kind === "proposals" ? "proposal" : kind === "takeovers" ? "takeover" : "grilling";
        const item = inbox.find(
          (each) =>
            each.kind.startsWith(itemKind) && each.data[key] === id && each.resolved_at === null,
        );
        if (!item)
          return fail(response, kind === "grillings" ? 422 : 409, `the ${key} is not open`);
        if (action === "confirm") {
          const issues = Array.isArray(body.issues)
            ? (body.issues as Array<{ title?: string }>)
            : [];
          if (issues.length === 0 || issues.some((issue) => !issue.title?.trim())) {
            return fail(response, 422, "every issue needs a title");
          }
        }
        item.resolved_at = ++clock;
        send({
          kind:
            kind === "proposals"
              ? "proposal_status"
              : kind === "takeovers"
                ? "takeover_status"
                : "grilling_ended",
          data: { [key]: id },
        });
        return ok(response, { id });
      }
      if (path === "/v1/changes/9/merge" && method === "POST") {
        resolveItem("ready_to_merge", "change", 9);
        send({ kind: "change_merged", data: { change: 9, commit: null } });
        return ok(response, { commit: "5ca1ab1e5ca1ab1e5ca1ab1e5ca1ab1e5ca1ab1" });
      }
      if (path === "/v1/runs/1" && method === "GET") {
        return ok(response, {
          id: 1,
          card_id: 1,
          agent: "Coder",
          harness: "pi-bolt",
          purpose: "code",
          status: "running",
          worktree: "/srv/.moebius/runs/1/worktree",
          started_at: Math.floor(Date.now() / 1000) - 90,
          ended_at: null,
          summary: null,
          repository_id: null,
          harness_repository: false,
          origin: "chat",
          runner: null,
        });
      }
      return false;
    },
  };

  seedInbox();
  return fake;
}
