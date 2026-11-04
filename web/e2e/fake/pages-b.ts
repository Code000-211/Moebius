/**
 * The routes of the Agents, Skills, MCP servers, Models and Plugins pages in the fake server, with the JSON the real
 * server sends (docs/reference/desktop-api.md: `GET /v1/agents`, the harness configuration routes, `GET /v1/models`
 * and the routes next to it, `GET /v1/plugins`, `GET` and `PUT /v1/settings`, `PUT /v1/secrets/{name}`). The state is
 * kept per server and changes the way the real one does: an edit of the harness configuration repository is a commit
 * with a short id, an invalid edit is refused with a sentence, a key typed on the Models page is stored under a name
 * and only the name is ever answered, and an OAuth login waits until the test finishes it.
 *
 * The routes the chat fake answers too (`GET /v1/agents`, `/v1/models/available`, `/v1/skills`) are left to it until
 * a test calls `use()`, so the other pages keep their data. A test can make the next request to a route fail with a
 * status and a sentence (`refuseNext`), and reads every request of these routes in `calls`.
 */
import type { IncomingMessage, ServerResponse } from "node:http";

type Json = unknown;
type Obj = Record<string, Json>;

export interface Call {
  method: string;
  path: string;
  /** The JSON body as received; the bytes of an upload are not kept. */
  body: Json;
}

export interface FakeAgent {
  settings: Obj;
  instructions: string;
  /** Why its files are broken. */
  broken?: string;
}

export interface PagesBFake {
  /** Answers one of these routes and returns true, or returns false for any other path. */
  handle(request: IncomingMessage, response: ServerResponse, path: string): Promise<boolean>;
  /** Takes over `GET /v1/agents`, `/v1/models/available` and `/v1/skills` from the chat fake. */
  use(): void;
  /** Whether the server has a harness configuration repository (it has, until `useWithoutHarness`). */
  harness: boolean;
  /** Starts without a harness configuration repository: agents are in `moebius.toml`. */
  useWithoutHarness(): void;
  readonly calls: Call[];
  /** The next request to `method path` fails with `status` and the sentence. */
  refuseNext(method: string, path: string, status: number, message: string): void;
  /** The zip files `POST /v1/uploads` took: their sizes. */
  readonly uploads: Array<{ id: string; size: number; type: string }>;
  /** The API keys `PUT /v1/secrets/{name}` stored, by name. A test reads them to see that a key was sent once. */
  readonly secrets: Record<string, string>;
  /** The keys of `moebius.toml` a running server uses only after a restart. */
  restartKeys: string[];
  /** What `POST /v1/server/restart` was asked, how many times. */
  restarts: number;
  /** Makes the OAuth login of the upstream at `place` end as logged in. */
  finishLogin(place: number): void;
  /** Makes it end as failed with the sentence. */
  failLogin(place: number, message: string): void;
  /** The models `GET /v1/models/available` lists. */
  availableErrors: Array<{ upstream: string; message: string }>;
  /** The state of the agents, skills and servers of the repository, for a test to read what an edit did. */
  readonly agents: Record<string, FakeAgent>;
  readonly skills: Record<
    string,
    { description: string; text: string; files: Record<string, string> }
  >;
  readonly mcp: Record<string, Obj>;
  /** The settings file as TOML would have it, as nested JSON. */
  readonly file: Obj;
}

// biome-ignore lint/suspicious/noTemplateCurlyInString: a reference, not a template
const DOCS_REF = "${DOCS_TOKEN}";

const SHORT = (n: number) => `c0ffee${n}`;

/** The keys of `moebius.toml` the Models page reads, with their defaults. */
const DEFAULTS: Record<string, Json> = {
  "router.upstreams": [],
  "router.classes": {},
  "teacher.model": null,
  "teacher.protocol": "openai",
  "teacher.timeout_seconds": 20,
  "decision_model.url": null,
  "decision_model.model": null,
  "decision_model.api_key_env": null,
  "decision_model.api_key_secret": null,
  "decision_model.timeout_seconds": 20,
  "decision_model.shadow": null,
  "decision_model.agreement": 0.95,
  "decision_model.min_judgements": 100,
  "harness.mcp_env": ["DOCS_TOKEN"],
  "harness.url": null,
  agents: {},
};

const BUILT_IN_SKILLS = [
  { name: "moebius", description: "How to operate and extend Moebius." },
  { name: "moebius-operate", description: "Operate a running Moebius." },
];
const WARNING =
  "Using a subscription through a third-party tool may break the provider's terms. Continue only if you accept that.";

const isObject = (value: Json): value is Obj =>
  value !== null && typeof value === "object" && !Array.isArray(value);

function readBytes(request: IncomingMessage): Promise<Buffer> {
  return new Promise((done) => {
    const chunks: Buffer[] = [];
    request.on("data", (chunk: Buffer) => chunks.push(chunk));
    request.on("end", () => done(Buffer.concat(chunks)));
  });
}

function send(response: ServerResponse, status: number, body: Json): true {
  if (typeof body === "string") {
    response.writeHead(status, { "content-type": "text/plain" });
    response.end(body);
  } else {
    response.writeHead(status, { "content-type": "application/json" });
    response.end(JSON.stringify(body));
  }
  return true;
}

/** Sets `value` at the dotted `key` of the nested `file`; a number segment addresses an array (one past the end adds). */
function setAt(file: Obj, key: string, value: Json): void {
  const parts = key.split(".");
  let node: Json = file;
  for (const [place, part] of parts.entries()) {
    const last = place === parts.length - 1;
    const holder = node as Obj | Json[];
    const index = Array.isArray(holder) ? Number(part) : part;
    if (last) {
      (holder as Obj)[index as string] = value;
      return;
    }
    const next = (holder as Obj)[index as string];
    if (next === undefined || next === null) {
      (holder as Obj)[index as string] = /^\d+$/.test(parts[place + 1] ?? "") ? [] : {};
    }
    node = (holder as Obj)[index as string];
  }
}

function removeAt(file: Obj, key: string): void {
  const parts = key.split(".");
  let node: Json = file;
  for (const part of parts.slice(0, -1)) {
    node = Array.isArray(node) ? node[Number(part)] : (node as Obj)?.[part];
    if (node === undefined || node === null) return;
  }
  const last = parts.at(-1) as string;
  if (Array.isArray(node)) node.splice(Number(last), 1);
  else delete (node as Obj)[last];
}

function getAt(file: Obj, key: string): Json | undefined {
  let node: Json = file;
  for (const part of key.split(".")) {
    if (node === undefined || node === null) return undefined;
    node = Array.isArray(node) ? node[Number(part)] : (node as Obj)[part];
  }
  return node;
}

export function createPagesBFake(): PagesBFake {
  let active = false;
  let commits = 0;
  let harnessPending = false;
  const calls: Call[] = [];
  const refusals = new Map<string, { status: number; message: string }>();
  const uploads: PagesBFake["uploads"] = [];
  const secrets: Record<string, string> = {};
  const logins = new Map<
    number,
    { state: string; message: string; code: boolean; loggedIn: boolean }
  >();
  const loggedIn = new Set<number>();
  const file: Obj = {
    router: {
      upstreams: [
        { protocol: "openai", base_url: "http://127.0.0.1:8080" },
        {
          protocol: "anthropic",
          base_url: "https://api.anthropic.com",
          api_key_secret: "anthropic",
        },
        {
          protocol: "anthropic",
          base_url: "https://api.subscription.example",
          oauth: { preset: "anthropic" },
        },
      ],
      classes: { strong: ["fake-coder", "fake-small"] },
    },
    teacher: { model: "fake-small" },
  };
  secrets.anthropic = "stored-before";
  const agents: Record<string, FakeAgent> = {
    Coder: {
      settings: {
        model: "fake-coder",
        protocol: "openai",
        thinking: "medium",
        skills: ["house-style"],
        extensions: [],
        mcp: ["docs"],
      },
      instructions: "Keep changes small.\n",
    },
    Reviewer: {
      settings: { model: "fake-small", protocol: "anthropic", skills: [], extensions: [], mcp: [] },
      instructions: "Review kindly.",
    },
    Broken: {
      settings: {},
      instructions: "",
      broken: "agents/Broken.toml: missing field `model`",
    },
  };
  const skills: PagesBFake["skills"] = {
    "house-style": {
      description: "Write plainly.",
      text: "Use short sentences.",
      files: { "scripts/check.py": "print('ok')\n", "assets/logo.bin": "\u0000binary" },
    },
  };
  const mcp: PagesBFake["mcp"] = {
    docs: {
      command: "docs-server",
      args: ["--root", "/srv/docs"],
      env: { TOKEN: DOCS_REF },
    },
    broken: { __error: "mcp.toml: `broken` has no command or url" },
  };
  const engineering: Record<
    string,
    { description: string; source: { url: string; commit: string } }
  > = {
    tdd: {
      description: "Test-driven development.",
      source: { url: "https://github.com/acme/skills", commit: "0123456789abcdef0123" },
    },
  };

  const fake: PagesBFake = {
    harness: true,
    calls,
    uploads,
    secrets,
    restartKeys: [],
    restarts: 0,
    availableErrors: [{ upstream: "https://api.broken.example", message: "401" }],
    agents,
    skills,
    mcp,
    file,
    use() {
      active = true;
    },
    useWithoutHarness() {
      active = true;
      fake.harness = false;
      // Agents live in the tables of moebius.toml.
      const table: Obj = {};
      for (const [name, agent] of Object.entries(agents)) {
        if (agent.broken) continue;
        table[name] = { ...agent.settings, instructions: agent.instructions };
      }
      file.agents = table;
    },
    refuseNext(method, path, status, message) {
      refusals.set(`${method} ${path}`, { status, message });
    },
    finishLogin(place) {
      loggedIn.add(place);
      logins.set(place, {
        state: "logged_in",
        message: "Logged in: the upstream lists 2 models.",
        code: false,
        loggedIn: true,
      });
    },
    failLogin(place, message) {
      logins.set(place, { state: "failed", message, code: false, loggedIn: false });
    },
    async handle(request, response, path) {
      const method = request.method ?? "GET";
      const url = new URL(request.url ?? "/", "http://x");
      const mine =
        /^\/v1\/(runners|agents|skills|mcp-servers|uploads|models|plugins|settings|secrets|harness|server\/restart)/.test(
          path,
        ) && !(path === "/v1/server");
      if (!mine) return false;
      // The Settings page's fake keeps the keys of the file, the restart and the setup until a test says otherwise.
      if (
        !active &&
        (path === "/v1/settings" || path === "/v1/server/restart" || path === "/v1/harness/setup")
      ) {
        return false;
      }
      // The chat fake answers these until a test says otherwise.
      if (
        !active &&
        method === "GET" &&
        (path === "/v1/agents" || path === "/v1/models/available" || path === "/v1/skills")
      ) {
        return false;
      }
      const bytes = method === "GET" ? Buffer.alloc(0) : await readBytes(request);
      const type = request.headers["content-type"] ?? "";
      if (path === "/v1/uploads") {
        calls.push({ method, path, body: null });
        const refused = refusals.get(`${method} ${path}`);
        if (refused) {
          refusals.delete(`${method} ${path}`);
          return send(response, refused.status, refused.message);
        }
        if (type !== "application/zip") return send(response, 415, "a zip file is needed");
        if (bytes.length === 0) return send(response, 422, "the file is empty");
        if (bytes.length > 20 * 1024 * 1024) return send(response, 413, "the file is too large");
        const id = "ab12cd34ab12cd34ab12cd34ab12cd34";
        uploads.push({ id, size: bytes.length, type });
        return send(response, 200, { id, path: `/srv/moebius/uploads/${id}.zip` });
      }
      let body: Json = null;
      if (bytes.length > 0) {
        try {
          body = JSON.parse(bytes.toString("utf8"));
        } catch {
          return send(response, 400, "the body is not valid JSON");
        }
      }
      calls.push({ method, path: path + url.search, body });
      const refusal = refusals.get(`${method} ${path}`);
      if (refusal) {
        refusals.delete(`${method} ${path}`);
        return send(response, refusal.status, refusal.message);
      }
      return route(method, path, url, body, request, response);
    },
  };

  const commit = (): string => {
    commits += 1;
    return SHORT(commits);
  };

  /** The agents the server lists: the Moebius agent and the Grilling partner first, then the others by name, the Coder
   * and the Reviewer always among them, built in, whether or not they have files. */
  function agentList(): Obj[] {
    const settingsOf = (name: string): Obj | undefined => {
      const own = fake.harness ? agents[name] : (file.agents as Obj | undefined)?.[name];
      return fake.harness ? (own as FakeAgent | undefined)?.settings : (own as Obj | undefined);
    };
    const entry = (name: string, settings: Obj | undefined, builtIn: boolean): Obj => ({
      name,
      model: String(settings?.model ?? ""),
      protocol: String(settings?.protocol ?? "openai"),
      ...(settings?.thinking ? { thinking: settings.thinking } : {}),
      ...(settings?.token_budget ? { token_budget: settings.token_budget } : {}),
      harness: String(settings?.harness ?? "pi-bolt"),
      built_in: builtIn,
    });
    const first = ["Moebius", "Grilling-partner"].map((name) =>
      entry(name, settingsOf(name) ?? settingsOf("Coder") ?? { model: "fake-coder" }, true),
    );
    const names = new Set<string>(["Coder", "Reviewer"]);
    for (const name of Object.keys(fake.harness ? agents : ((file.agents as Obj) ?? {})))
      names.add(name);
    const others = [...names]
      .filter((name) => name !== "Moebius" && name !== "Grilling-partner")
      .map((name) => {
        const builtIn = name === "Coder" || name === "Reviewer";
        const broken = fake.harness ? agents[name]?.broken : undefined;
        if (broken) {
          return { ...entry(name, undefined, false), error: broken };
        }
        // The Reviewer without a model of its own takes the Coder's; the Coder has no one to take from.
        const settings =
          settingsOf(name) ?? (name === "Reviewer" ? settingsOf("Coder") : undefined);
        return entry(name, settings, builtIn);
      });
    return [...first, ...others.sort((a, b) => String(a.name).localeCompare(String(b.name)))];
  }

  function builtInPart(name: string): Obj {
    // The Coder and the Reviewer have no reference library, only the engineering skills.
    const coding = name === "Coder" || name === "Reviewer";
    return {
      instructions: `You are the ${name}${coding ? "." : " agent."} Moebius brings these instructions.`,
      skills: coding ? [] : BUILT_IN_SKILLS,
      engineering_skills:
        (
          { Coder: ["tdd", "diagnosing-bugs"], Reviewer: ["code-review"] } as Record<
            string,
            string[]
          >
        )[name]
          ?.concat("moebius-autonomous")
          .map((skill) => ({ name: skill, description: `The ${skill} skill.` })) ?? [],
    };
  }

  function skillList(): Obj[] {
    if (!fake.harness) return [];
    return [
      ...BUILT_IN_SKILLS.map((skill) => ({ ...skill, built_in: true })),
      ...Object.entries(engineering).map(([name, each]) => ({
        name,
        description: each.description,
        built_in: true,
        source: each.source,
      })),
      ...Object.entries(skills)
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([name, skill]) => ({ name, description: skill.description, built_in: false })),
    ];
  }

  function skillDetail(name: string, committed?: string): Obj | undefined {
    const own = skills[name];
    if (own) {
      return {
        name,
        description: own.description,
        text: own.text,
        files: Object.keys(own.files).sort(),
        ...(committed ? { commit: committed } : {}),
      };
    }
    const builtIn = BUILT_IN_SKILLS.find((skill) => skill.name === name);
    if (builtIn)
      return { name, description: builtIn.description, text: `The ${name} skill.`, files: [] };
    const copied = engineering[name];
    if (copied) {
      return {
        name,
        description: copied.description,
        text: `The ${name} skill.`,
        files: [],
        source: copied.source,
      };
    }
    return undefined;
  }

  const namesAgentsUsing = (key: "skills" | "mcp", name: string) =>
    Object.entries(agents)
      .filter(([, agent]) => (agent.settings[key] as string[] | undefined)?.includes(name))
      .map(([agentName]) => agentName);

  function settingValues(): Obj {
    const values: Obj = {};
    for (const [key, fallback] of Object.entries(DEFAULTS)) {
      const found = getAt(file, key);
      values[key] =
        found === undefined ? { value: fallback, default: true } : { value: found, default: false };
    }
    return values;
  }

  function upstreams(): Obj[] {
    const list = getAt(file, "router.upstreams");
    return Array.isArray(list) ? (list as Obj[]) : [];
  }

  function validateSettings(): string | null {
    for (const upstream of upstreams()) {
      const secret = upstream.api_key_secret;
      if (typeof secret === "string" && !(secret in secrets)) {
        return `the upstream names the secret ${secret}, which is not stored`;
      }
    }
    const decision = getAt(file, "decision_model.api_key_secret");
    if (typeof decision === "string" && !(decision in secrets)) {
      return `the decision model names the secret ${decision}, which is not stored`;
    }
    for (const upstream of upstreams()) {
      if (typeof upstream.base_url !== "string" || upstream.base_url === "")
        return "router.upstreams: base_url is needed";
    }
    return null;
  }

  function oauthOf(place: number) {
    const upstream = upstreams()[place];
    return upstream && isObject(upstream.oauth) ? (upstream.oauth as Obj) : null;
  }

  function loginOf(place: number, origin: string): Obj {
    const kept = logins.get(place);
    const oauth = oauthOf(place) as Obj;
    const flow = oauth.flow === "device" ? "device" : "code";
    const state = kept?.state ?? "logged_out";
    return {
      state,
      flow,
      user_code: flow === "device" && state === "waiting" ? "WDJB-MJHT" : null,
      verification_uri:
        flow === "device" && state === "waiting" ? "https://login.example.com/device" : null,
      verification_uri_complete: null,
      authorize_url:
        flow === "code" && state === "waiting"
          ? `${origin}/fake-provider/authorize?state=fake-state`
          : null,
      message:
        kept?.message ??
        (flow === "device"
          ? "Open https://login.example.com/device and enter the code WDJB-MJHT."
          : "Open the provider's page and log in."),
    };
  }

  function route(
    method: string,
    path: string,
    url: URL,
    body: Json,
    request: IncomingMessage,
    response: ServerResponse,
  ): true {
    const origin = `http://${request.headers.host ?? "127.0.0.1"}`;
    const input = (isObject(body) ? body : {}) as Obj;

    if (path === "/v1/runners" && method === "GET") {
      return send(response, 200, [
        { name: "box", host: "build.example.com", user: "ci", max_runs: 2, busy: 0 },
      ]);
    }

    // --- agents ---
    if (path === "/v1/agents" && method === "GET") return send(response, 200, agentList());
    const agentRoute = /^\/v1\/agents\/([^/]+)$/.exec(path);
    if (agentRoute) {
      const name = decodeURIComponent(agentRoute[1] as string);
      const builtIn =
        name === "Moebius" ||
        name === "Grilling-partner" ||
        name === "Coder" ||
        name === "Reviewer";
      if (method === "GET") {
        if (builtIn) {
          const own = agents[name];
          return send(response, 200, {
            name,
            ...(own && fake.harness
              ? { settings: own.settings, instructions: own.instructions }
              : {}),
            built_in: builtInPart(name),
          });
        }
        if (!fake.harness)
          return send(response, 422, "the server has no [harness]; its agents are in moebius.toml");
        const agent = agents[name];
        if (!agent) return send(response, 404, `agent ${name} not found`);
        if (agent.broken) return send(response, 422, agent.broken);
        return send(response, 200, {
          name,
          settings: agent.settings,
          instructions: agent.instructions,
        });
      }
      if (!fake.harness) return send(response, 422, "the server has no [harness]");
      if (method === "PUT") {
        if (!/^[A-Za-z0-9_-]+$/.test(name))
          return send(response, 422, `invalid agent name ${name}`);
        const settings = input.settings;
        if (!isObject(settings) || typeof settings.model !== "string" || settings.model === "") {
          return send(response, 422, "settings.model is needed");
        }
        const instructions = typeof input.instructions === "string" ? input.instructions : "";
        const before = JSON.stringify(agents[name]);
        agents[name] = { settings, instructions };
        const changed = before !== JSON.stringify(agents[name]);
        return send(response, 200, {
          name,
          settings,
          instructions,
          ...(changed ? { commit: commit() } : {}),
        });
      }
      if (method === "DELETE") {
        if (!agents[name]) return send(response, 404, `agent ${name} not found`);
        delete agents[name];
        return send(response, 200, null);
      }
    }

    // --- skills ---
    if (path === "/v1/skills" && method === "GET") return send(response, 200, skillList());
    const fileRoute = /^\/v1\/skills\/([^/]+)\/files\/(.+)$/.exec(path);
    if (fileRoute) {
      const name = decodeURIComponent(fileRoute[1] as string);
      const rel = (fileRoute[2] ?? "").split("/").map(decodeURIComponent).join("/");
      if (!fake.harness) return send(response, 409, "the server has no [harness]");
      const skill = skills[name];
      if (!skill && !skillDetail(name)) return send(response, 404, `skill ${name} not found`);
      if (rel === "SKILL.md" || rel.split("/").includes("..")) {
        return send(response, 422, `the path ${rel} is not one of the skill's other files`);
      }
      if (method === "GET") {
        const text = skill?.files[rel];
        if (text === undefined) return send(response, 404, `file ${rel} not found`);
        if (text.includes("\u0000"))
          return send(response, 200, { path: rel, size: 20480, binary: true });
        return send(response, 200, { path: rel, size: text.length, text });
      }
      if (!skill) return send(response, 409, `the skill ${name} is built in and read only`);
      if (method === "PUT") {
        const text = typeof input.text === "string" ? input.text : "";
        const changed = skill.files[rel] !== text;
        skill.files[rel] = text;
        return send(response, 200, skillDetail(name, changed ? commit() : undefined));
      }
      if (method === "DELETE") {
        if (!(rel in skill.files)) return send(response, 404, `file ${rel} not found`);
        delete skill.files[rel];
        return send(response, 200, skillDetail(name, commit()));
      }
    }
    const skillRoute = /^\/v1\/skills\/([^/]+)$/.exec(path);
    if (skillRoute) {
      const name = decodeURIComponent(skillRoute[1] as string);
      if (!fake.harness) return send(response, 409, "the server has no [harness]");
      if (method === "GET") {
        const detail = skillDetail(name);
        return detail
          ? send(response, 200, detail)
          : send(response, 404, `skill ${name} not found`);
      }
      const builtIn = !skills[name] && skillDetail(name) !== undefined;
      if (method === "PUT") {
        if (builtIn || name === "moebius")
          return send(response, 409, `the skill ${name} is built in and read only`);
        if (!/^[A-Za-z0-9_-]+$/.test(name))
          return send(response, 422, `invalid skill name ${name}`);
        const description = typeof input.description === "string" ? input.description.trim() : "";
        if (description === "" || description.includes("\n")) {
          return send(response, 422, "the description is one line and not blank");
        }
        const text = typeof input.text === "string" ? input.text : "";
        const before = skills[name];
        const changed = !before || before.description !== description || before.text !== text;
        skills[name] = { description, text, files: before?.files ?? {} };
        return send(response, 200, skillDetail(name, changed ? commit() : undefined));
      }
      if (method === "DELETE") {
        if (builtIn) return send(response, 409, `the skill ${name} is built in and read only`);
        if (!skills[name]) return send(response, 404, `skill ${name} not found`);
        const using = namesAgentsUsing("skills", name);
        if (using.length > 0)
          return send(response, 409, `the skill ${name} is named by ${using.join(", ")}`);
        delete skills[name];
        return send(response, 200, null);
      }
    }

    // --- MCP servers ---
    if (path === "/v1/mcp-servers" && method === "GET") {
      if (!fake.harness) return send(response, 200, []);
      return send(
        response,
        200,
        Object.entries(mcp).map(([name, server]) =>
          typeof server.__error === "string"
            ? { name, kind: "unknown", error: server.__error }
            : { name, kind: server.url ? "remote" : "stdio" },
        ),
      );
    }
    const mcpRoute = /^\/v1\/mcp-servers\/([^/]+)$/.exec(path);
    if (mcpRoute) {
      const name = decodeURIComponent(mcpRoute[1] as string);
      if (!fake.harness) return send(response, 409, "the server has no [harness]");
      if (method === "GET") {
        const server = mcp[name];
        if (!server) return send(response, 404, `MCP server ${name} not found`);
        if (typeof server.__error === "string") return send(response, 422, server.__error);
        return send(response, 200, { name, ...server });
      }
      if (method === "PUT") {
        const wanted = Object.keys(input);
        const stdio = wanted.every((key) => ["command", "args", "env"].includes(key));
        const remote = wanted.every((key) => ["url", "headers"].includes(key));
        if ("command" in input === "url" in input || !(stdio || remote)) {
          return send(
            response,
            422,
            `mcp.toml: ${name} needs one of command and url, and only the keys that belong to it`,
          );
        }
        for (const value of [
          ...Object.values((input.env as Obj) ?? {}),
          ...Object.values((input.headers as Obj) ?? {}),
        ]) {
          for (const reference of String(value).matchAll(/\$\{([A-Z_]+)\}/g)) {
            const allowed = (getAt(file, "harness.mcp_env") as string[] | undefined) ?? [
              "DOCS_TOKEN",
            ];
            if (!allowed.includes(reference[1] as string)) {
              return send(
                response,
                422,
                `mcp.toml: ${name} uses \${${reference[1]}}, which is not in harness.mcp_env`,
              );
            }
          }
        }
        const changed = JSON.stringify(mcp[name]) !== JSON.stringify(input);
        mcp[name] = input;
        return send(response, 200, { name, ...input, ...(changed ? { commit: commit() } : {}) });
      }
      if (method === "DELETE") {
        if (!mcp[name]) return send(response, 404, `MCP server ${name} not found`);
        const using = namesAgentsUsing("mcp", name);
        if (using.length > 0)
          return send(response, 409, `the MCP server ${name} is named by ${using.join(", ")}`);
        delete mcp[name];
        return send(response, 200, null);
      }
    }

    // --- setting the repository up, restarting ---
    if (path === "/v1/harness/setup" && method === "POST") {
      if (fake.harness || harnessPending)
        return send(response, 409, "moebius.toml has [harness] already");
      harnessPending = true;
      fake.restartKeys = ["harness.url"];
      return send(response, 200, { restart_needed: true, restart_keys: ["harness.url"] });
    }
    if (path === "/v1/server/restart" && method === "POST") {
      fake.restarts += 1;
      fake.restartKeys = [];
      if (harnessPending) {
        harnessPending = false;
        fake.harness = true;
        const table = (file.agents as Obj | undefined) ?? {};
        for (const [name, value] of Object.entries(table)) {
          const { instructions, ...settings } = value as Obj;
          agents[name] = {
            settings: { skills: [], extensions: [], mcp: [], ...settings },
            instructions: String(instructions ?? ""),
          };
        }
        delete file.agents;
      }
      return send(response, 200, null);
    }

    // --- models ---
    if (path === "/v1/models" && method === "GET") {
      const teacher = getAt(file, "teacher");
      const decision = getAt(file, "decision_model");
      return send(response, 200, {
        upstreams: upstreams().map((upstream, place) => {
          const oauth = isObject(upstream.oauth) ? (upstream.oauth as Obj) : null;
          return {
            protocol: upstream.protocol,
            base_url: upstream.base_url,
            api_key_set: Boolean(upstream.api_key_env || upstream.api_key_secret),
            oauth: oauth && {
              flow: oauth.flow === "device" ? "device" : "code",
              logged_in: loggedIn.has(place),
              warning: WARNING,
              preset: typeof oauth.preset === "string" ? oauth.preset : null,
              needs: null,
            },
          };
        }),
        agents: agentList().map((agent) => ({ ...agent })),
        teacher: isObject(teacher)
          ? {
              model: teacher.model,
              protocol: teacher.protocol ?? "openai",
              timeout_seconds: teacher.timeout_seconds ?? 20,
            }
          : null,
        decision_model: isObject(decision)
          ? {
              url: decision.url ?? "",
              model: decision.model ?? "",
              timeout_seconds: decision.timeout_seconds ?? 20,
              api_key_set: Boolean(decision.api_key_env || decision.api_key_secret),
            }
          : null,
      });
    }
    if (path === "/v1/models/available" && method === "GET") {
      const withDecision =
        url.searchParams.get("decision") === "true" && isObject(getAt(file, "decision_model"));
      return send(response, 200, {
        models: [
          { id: "fake-coder", protocol: "openai", upstream: "http://127.0.0.1:8080" },
          { id: "fake-small", protocol: "openai", upstream: "http://127.0.0.1:8080" },
          { id: "claude-fake", protocol: "anthropic", upstream: "https://api.anthropic.com" },
        ],
        errors: fake.availableErrors,
        classes: [
          {
            name: "strong",
            models: ["fake-coder", "claude-fake"],
            protocols: ["openai", "anthropic"],
          },
        ],
        decision_models: withDecision ? ["moebius-decider"] : [],
      });
    }
    if (path === "/v1/models/providers" && method === "GET") {
      return send(response, 200, [
        {
          name: "anthropic",
          title: "Anthropic (Claude Pro/Max)",
          protocol: "anthropic",
          base_url: "https://api.anthropic.com",
          needs: null,
        },
        {
          name: "openai",
          title: "OpenAI (ChatGPT)",
          protocol: "openai",
          base_url: "https://api.openai.com",
          needs: null,
        },
      ]);
    }
    const loginRoute = /^\/v1\/models\/upstreams\/(\d+)\/login(\/code)?$/.exec(path);
    if (loginRoute) {
      const place = Number(loginRoute[1]);
      if (!oauthOf(place)) {
        return send(
          response,
          upstreams()[place] ? 422 : 404,
          upstreams()[place] ? "the upstream has no oauth table" : "no such upstream",
        );
      }
      if (loginRoute[2] && method === "POST") {
        const kept = logins.get(place);
        if (kept?.state !== "waiting" || !kept.code)
          return send(response, 409, "the upstream has no login waiting for a code");
        const pasted = typeof input.pasted === "string" ? input.pasted.trim() : "";
        if (pasted === "" || pasted === "bad")
          return send(response, 422, "nothing in pasted is a code");
        // The exchange goes on in the background; the next read tells how it ended.
        logins.set(place, {
          state: "logged_in",
          message: "Logged in: the upstream lists 2 models.",
          code: false,
          loggedIn: true,
        });
        loggedIn.add(place);
        return send(response, 200, { ...loginOf(place, origin), state: "waiting" });
      }
      if (method === "POST") {
        if (input.accept_terms !== true) return send(response, 422, "accept_terms must be true");
        const oauth = oauthOf(place) as Obj;
        const code = oauth.flow !== "device";
        logins.set(place, { state: "waiting", message: "", code, loggedIn: false });
        const started = loginOf(place, origin);
        logins.set(place, {
          state: "waiting",
          message: String(started.message),
          code,
          loggedIn: false,
        });
        return send(response, 200, started);
      }
      if (method === "GET") {
        const kept = logins.get(place);
        if (!kept) {
          return send(response, 200, {
            ...loginOf(place, origin),
            state: loggedIn.has(place) ? "logged_in" : "logged_out",
            message: loggedIn.has(place)
              ? "Logged in: the upstream lists 2 models."
              : "Not logged in.",
          });
        }
        return send(response, 200, loginOf(place, origin));
      }
    }

    // --- plugins ---
    if (path === "/v1/plugins" && method === "GET") {
      return send(response, 200, {
        time_limit_seconds: 30,
        plugins: [
          {
            name: "guard",
            version: "0.1.0",
            kinds: ["decider"],
            decides: ["tool_call"],
            process: null,
          },
          {
            name: "allow-reads",
            version: "0.1.0",
            kinds: ["decider"],
            decides: ["start_card"],
            process: {
              program: "/srv/plugins/allow-reads",
              state: "running",
              restarts: 2,
              error: "exited with status 1",
              log: "/srv/.moebius/plugins/allow-reads.log",
            },
          },
          { name: "idle", version: "", kinds: [], decides: [], process: null },
        ],
      });
    }

    // --- settings and secrets ---
    if (path === "/v1/settings" && method === "GET") {
      return send(response, 200, {
        file: "/srv/moebius/moebius.toml",
        values: settingValues(),
        secrets: Object.keys(secrets),
        restart_needed: fake.restartKeys.length > 0,
        restart_keys: fake.restartKeys,
      });
    }
    if (path === "/v1/settings" && method === "PUT") {
      const before = structuredClone(file);
      for (const key of (input.remove as string[] | undefined) ?? []) removeAt(file, key);
      for (const [key, value] of Object.entries((input.set as Obj | undefined) ?? {}))
        setAt(file, key, value);
      const problem = validateSettings();
      if (problem) {
        for (const key of Object.keys(file)) delete file[key];
        Object.assign(file, before);
        return send(response, 422, problem);
      }
      return send(response, 200, {
        restart_needed: fake.restartKeys.length > 0,
        restart_keys: fake.restartKeys,
      });
    }
    const secretRoute = /^\/v1\/secrets\/([^/]+)$/.exec(path);
    if (secretRoute && method === "PUT") {
      const name = decodeURIComponent(secretRoute[1] as string);
      const value = typeof input.value === "string" ? input.value : "";
      if (!/^[A-Za-z0-9_-]+$/.test(name) || value.trim() === "")
        return send(response, 422, "the name or the key is not allowed");
      secrets[name] = value;
      return send(response, 200, Object.keys(secrets));
    }
    if (secretRoute && method === "DELETE") {
      const name = decodeURIComponent(secretRoute[1] as string);
      if (!(name in secrets)) return send(response, 404, `secret ${name} not found`);
      delete secrets[name];
      return send(response, 200, Object.keys(secrets));
    }
    // A route this fake does not know (the chat's and the board's are answered elsewhere).
    return send(response, 404, "no such route");
  }

  return fake;
}
