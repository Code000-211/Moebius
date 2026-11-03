/**
 * The routes of the package managers in the fake server, with the JSON the real one sends (docs/reference/
 * desktop-api.md, "Packages" and the `/v1/extensions` routes): the Moebius registries (`/v1/packages`) and the pi.dev
 * catalog (`/v1/extensions`). A plugin is installed at once; a package of files and every Pi package is a change the
 * fake offers, with a `ready_to_merge` item in the inbox the way the real server does. The state changes the way the
 * real one does and the events it sends are the real ones. A test can make a registry unreadable (`problem`), make the
 * next request to a route fail with the server's own sentence (`refuseNext`), and read every request in `calls`.
 */
import type { IncomingMessage, ServerResponse } from "node:http";
import type { PagesAFake } from "./pages-a";

type Send = (event: { kind: string; data: unknown }) => void;
type Obj = Record<string, unknown>;

export interface PackagesFake {
  /** Answers one of these routes and returns true, or returns false for any other path. */
  handle(request: IncomingMessage, response: ServerResponse, path: string): Promise<boolean>;
  /** Every request made to these routes. */
  readonly calls: Array<{ method: string; path: string; body: unknown }>;
  /** Makes the next request with `method` to `path` fail with `status` and `message`. */
  refuseNext(method: string, path: string, status: number, message: string): void;
  /** A registry the search cannot read, said beside the results of the others; null for none. */
  problem: { registry: string; message: string } | null;
  /** Puts the nightly check's update of an installed Pi package in the inbox. */
  waitForUpdate(name: string, from: string, to: string): void;
  /** The names of the packages installed from the registries, in order. */
  installedNames(): string[];
}

interface Entry {
  name: string;
  kind: "plugin" | "workflow" | "rules" | "skills";
  versions: Array<{ version: string; moebius: string; compatible: boolean }>;
  description: string;
  restart?: boolean;
  /** Why the plugin does not run after its install. */
  broken?: string;
  /** The sentence that refuses its install. */
  refuses?: { status: number; message: string };
}

const REGISTRY = "moebius-plugins";

const ENTRIES: Entry[] = [
  {
    name: "gate",
    kind: "plugin",
    versions: [
      { version: "1.1.0", moebius: ">=0.1", compatible: true },
      { version: "1.0.0", moebius: ">=0.1", compatible: true },
    ],
    description: "Holds cards marked wip.",
  },
  {
    name: "guard-pack",
    kind: "plugin",
    versions: [{ version: "2.0.0", moebius: ">=0.1", compatible: true }],
    description: "Checks every tool call against a list.",
  },
  {
    name: "pad-launcher",
    kind: "plugin",
    versions: [{ version: "0.4.0", moebius: ">=0.1", compatible: true }],
    description: "Starts harnesses in a container.",
    restart: true,
  },
  {
    name: "shaky",
    kind: "plugin",
    versions: [{ version: "0.1.0", moebius: ">=0.1", compatible: true }],
    description: "A plugin that does not start.",
    broken: "exited with status 1 before sending its manifest",
  },
  {
    name: "from-the-future",
    kind: "plugin",
    versions: [{ version: "9.0.0", moebius: ">=9.0", compatible: false }],
    description: "Works with a Moebius that is not out yet.",
    refuses: {
      status: 422,
      message: "from-the-future 9.0.0 works with Moebius >=9.0, and this is Moebius 0.0.0-fake",
    },
  },
  {
    name: "clash",
    kind: "plugin",
    versions: [{ version: "1.0.0", moebius: ">=0.1", compatible: true }],
    description: "Has the name of a plugin the server already loads.",
    refuses: {
      status: 422,
      message: "a plugin named clash is already named in moebius.toml",
    },
  },
  {
    name: "review-flow",
    kind: "workflow",
    versions: [{ version: "1.0.0", moebius: ">=0.1", compatible: true }],
    description: "A review column for every card.",
  },
  {
    name: "house-rules",
    kind: "rules",
    versions: [{ version: "1.2.0", moebius: ">=0.1", compatible: true }],
    description: "Rules every repository wants.",
  },
  {
    name: "style-skills",
    kind: "skills",
    versions: [{ version: "0.3.0", moebius: ">=0.1", compatible: true }],
    description: "Skills for writing in the house style.",
  },
];

interface Hit {
  name: string;
  version: string;
  description: string;
  kinds: string[];
  weekly_downloads: number | null;
}

const HITS: Hit[] = [
  {
    name: "pi-web-access",
    version: "1.4.0",
    description: "Lets an agent fetch web pages.",
    kinds: ["extension"],
    weekly_downloads: 12400,
  },
  {
    name: "@acme/pi-notes",
    version: "0.4.0",
    description: "Notes an agent keeps between runs.",
    kinds: ["extension", "skill"],
    weekly_downloads: 830,
  },
  {
    name: "pi-midnight",
    version: "2.0.1",
    description: "A dark theme.",
    kinds: ["theme"],
    weekly_downloads: null,
  },
];

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

function readBody(request: IncomingMessage): Promise<unknown> {
  return new Promise((done) => {
    const chunks: Buffer[] = [];
    request.on("data", (chunk: Buffer) => chunks.push(chunk));
    request.on("end", () => {
      const text = Buffer.concat(chunks).toString("utf8");
      done(text === "" ? null : JSON.parse(text));
    });
  });
}

export function createPackagesFake(send: Send, pagesA: PagesAFake): PackagesFake {
  const calls: PackagesFake["calls"] = [];
  const refusals = new Map<string, { status: number; message: string }>();
  let change = 40;
  // The lock: what is installed from the registries.
  const lock: Array<{ name: string; version: string; kind: Entry["kind"] }> = [
    { name: "gate", version: "1.0.0", kind: "plugin" },
    { name: "guard-pack", version: "2.0.0", kind: "plugin" },
  ];
  // What is installed as a Pi package; a source other than npm's is given, as for one added from a git repository.
  const piLock: Array<{ name: string; version: string; agents: string[]; source?: string }> = [
    { name: "pi-notes", version: "0.3.0", agents: ["Coder"] },
    {
      name: "pi-gitted",
      version: "0.1.0",
      agents: [],
      source: "git:github.com/acme/pi-gitted",
    },
  ];
  const sourceOf = (each: { name: string; source?: string }) => each.source ?? `npm:${each.name}`;

  const entry = (name: string) => ENTRIES.find((each) => each.name === name);
  const newest = (each: Entry) => each.versions.find((v) => v.compatible) ?? each.versions[0];
  const locked = (name: string) => lock.find((each) => each.name === name);
  const offer = (action: "add" | "update" | "remove", name: string, extra: Obj): Obj => {
    change += 1;
    const branch = `moebius/ext-${action}-${name.replace(/\W/g, "-")}-${change}`;
    pagesA.addInboxItem({
      kind: "ready_to_merge",
      title: `${action === "add" ? "Install" : action === "update" ? "Update" : "Remove"} ${name}`,
      data: { change, branch },
    });
    return {
      change: {
        id: change,
        repository_id: null,
        harness_repository: true,
        card_id: null,
        branch,
        state: "open",
        pull_request: null,
        checks: "pending",
        review: null,
        head: null,
        published_at: null,
      },
      package: name,
      action,
      source: `npm:${name}`,
      from: null,
      to: null,
      resolved: null,
      integrity: null,
      resources: { extensions: [], skills: [], prompts: [], themes: [] },
      agents: [],
      changelog: "",
      files: [],
      dependency_files: 0,
      diff: "",
      ...extra,
    };
  };
  const outcome = (each: Entry | undefined, name: string, version: string, action: string) => ({
    name,
    version,
    kind: each?.kind ?? "plugin",
    action,
    restart_needed: each?.restart === true && action !== "removed",
  });

  async function route(
    method: string,
    path: string,
    query: URLSearchParams,
    body: Obj,
    response: ServerResponse,
  ): Promise<boolean> {
    // --- the Moebius registries ---
    if (path === "/v1/packages/search" && method === "GET") {
      const text = (query.get("q") ?? "").toLowerCase();
      const packages = ENTRIES.filter((each) =>
        `${each.name} ${each.description} ${each.kind}`.toLowerCase().includes(text),
      ).map((each) => ({
        name: each.name,
        kind: each.kind,
        version: newest(each)?.version,
        description: each.description,
        registry: REGISTRY,
        compatible: newest(each)?.compatible === true,
        installed: locked(each.name)?.version ?? null,
      }));
      return json(response, 200, { packages, problems: fake.problem ? [fake.problem] : [] });
    }
    if (path === "/v1/packages" && method === "GET") {
      return json(
        response,
        200,
        lock.map((each) => {
          const top = entry(each.name);
          const best = top ? newest(top)?.version : undefined;
          return {
            ...each,
            registry: REGISTRY,
            url: "",
            sha256: "",
            update_available: best && best !== each.version ? best : null,
          };
        }),
      );
    }
    if (path === "/v1/packages" && method === "POST") {
      const name = String(body.name);
      const each = entry(name);
      if (!each) return json(response, 404, `no package ${name} in the registries`);
      if (each.refuses) return json(response, each.refuses.status, each.refuses.message);
      if (locked(name))
        return json(response, 409, `${name} is installed already; update it instead`);
      const version = String(newest(each)?.version);
      if (each.kind === "plugin") {
        lock.push({ name, version, kind: each.kind });
        send({ kind: "package_installed", data: { name, version, kind: each.kind } });
        return json(response, 200, {
          ...outcome(each, name, version, "installed"),
          plugin: {
            program: `/srv/.moebius/packages/${name}/${version}/${name}`,
            state: each.broken ? "restarting" : "running",
            restarts: each.broken ? 1 : 0,
            error: each.broken ?? null,
          },
        });
      }
      const made = offer("add", name, {});
      lock.push({ name, version, kind: each.kind });
      send({ kind: "package_installed", data: { name, version, kind: each.kind } });
      return json(response, 200, {
        ...outcome(each, name, version, "installed"),
        change: (made.change as { id: number }).id,
      });
    }
    const named = /^\/v1\/packages\/([^/]+)(\/update)?$/.exec(path);
    if (named && path !== "/v1/packages/search") {
      const name = decodeURIComponent(named[1] as string);
      const each = entry(name);
      if (method === "GET" && !named[2]) {
        if (!each) return json(response, 404, `no package ${name} in the registries`);
        const held = locked(name);
        return json(response, 200, {
          name,
          registry: REGISTRY,
          installed: held ? { ...held, registry: REGISTRY, url: "", sha256: "" } : null,
          versions: each.versions.map((v) => ({
            name,
            kind: each.kind,
            description: each.description,
            license: "MIT",
            homepage: `https://example.test/${name}`,
            moebius: v.moebius,
            compatible: v.compatible,
            ...(each.kind === "plugin"
              ? { plugin: { kind: "decider", program: name, decides: ["start_card"] } }
              : { files: [{ to: `${name}.luau` }] }),
            version: v.version,
          })),
        });
      }
      const held = locked(name);
      if (method === "POST" && named[2]) {
        if (!held) return json(response, 404, `${name} is not installed`);
        const best = each ? newest(each)?.version : held.version;
        if (!best || best === held.version) {
          return json(response, 200, outcome(each, name, held.version, "unchanged"));
        }
        held.version = best;
        send({ kind: "package_installed", data: { name, version: best, kind: held.kind } });
        return json(response, 200, outcome(each, name, best, "updated"));
      }
      if (method === "DELETE" && !named[2]) {
        if (!held) return json(response, 404, `${name} is not installed`);
        lock.splice(lock.indexOf(held), 1);
        send({ kind: "package_removed", data: { name, kind: held.kind } });
        return json(response, 200, outcome(each, name, held.version, "removed"));
      }
    }

    // --- the pi.dev catalog ---
    if (path === "/v1/extensions/search" && method === "GET") {
      const text = (query.get("q") ?? "").toLowerCase();
      const type = query.get("type");
      if (type && !["extension", "skill", "theme", "prompt"].includes(type)) {
        return json(response, 422, `${type} is not a type of package`);
      }
      return json(
        response,
        200,
        HITS.filter(
          (hit) =>
            `${hit.name} ${hit.description}`.toLowerCase().includes(text) &&
            (type === null || hit.kinds.includes(type)),
        ).map((hit) => ({
          ...hit,
          npm: `https://www.npmjs.com/package/${hit.name}`,
          homepage: null,
          repository: null,
          date: null,
        })),
      );
    }
    if (path === "/v1/extensions/info" && method === "GET") {
      const source = query.get("source") ?? "";
      const held = piLock.find((each) => sourceOf(each) === source);
      const name = held?.name ?? source.replace(/^npm:/, "");
      const hit = HITS.find((each) => each.name === name);
      if (!hit && !held) return json(response, 404, `no package ${source}`);
      return json(response, 200, {
        name,
        source,
        version: hit?.version ?? held?.version,
        resolved: `https://registry.npmjs.org/${name}/-/${name}-${hit?.version}.tgz`,
        integrity: "sha512-fake",
        description: hit?.description ?? "",
        resources: {
          extensions: hit?.kinds.includes("extension") ? ["extensions/main.ts"] : [],
          skills: hit?.kinds.includes("skill") ? ["skills/notes"] : [],
          prompts: [],
          themes: hit?.kinds.includes("theme") ? ["themes/midnight.json"] : [],
        },
        dependencies: name === "pi-web-access" ? ["undici"] : [],
        install_scripts: name === "pi-web-access" ? ["postinstall"] : [],
        files: 7,
        installed: held?.version ?? null,
      });
    }
    if (path === "/v1/extensions/packages" && method === "GET") {
      return json(
        response,
        200,
        piLock.map((each) => ({
          name: each.name,
          source: sourceOf(each),
          version: each.version,
          resolved: "",
          integrity: null,
          sha256: null,
          resources: { extensions: ["extensions/main.ts"], skills: [], prompts: [], themes: [] },
          install_scripts: [],
          agents: each.agents,
          error: null,
        })),
      );
    }
    if (path === "/v1/extensions/packages" && method === "POST") {
      const source = String(body.source);
      const name = source.replace(/^npm:/, "");
      const hit = HITS.find((each) => each.name === name);
      if (!hit) return json(response, 404, `no package ${name} on npm`);
      if (piLock.some((each) => each.name === name)) {
        return json(response, 409, `${name} is installed already`);
      }
      const agent = typeof body.agent === "string" ? body.agent : null;
      return json(
        response,
        200,
        offer("add", name, {
          source,
          to: hit.version,
          agents: agent ? [agent] : [],
          files: [`A packages/${name}/package.json`],
        }),
      );
    }
    const piNamed = /^\/v1\/extensions\/packages\/([^/]+)(\/update)?$/.exec(path);
    if (piNamed) {
      const name = decodeURIComponent(piNamed[1] as string);
      const held = piLock.find((each) => each.name === name);
      if (!held) return json(response, 404, `${name} is not installed`);
      if (method === "POST" && piNamed[2]) {
        const hit = HITS.find((each) => each.name === name) ?? { version: "0.4.0" };
        if (hit.version === held.version) {
          return json(response, 200, { offered: [], up_to_date: [name], failed: [] });
        }
        return json(response, 200, {
          offered: [
            offer("update", name, {
              from: held.version,
              to: hit.version,
              changelog: "## 0.4.0\n\n- Notes survive a restart.\n",
              files: [`M packages/${name}/package.json`],
            }),
          ],
          up_to_date: [],
          failed: [],
        });
      }
      if (method === "DELETE" && !piNamed[2]) {
        return json(
          response,
          200,
          offer("remove", name, { from: held.version, agents: held.agents }),
        );
      }
    }
    if (path === "/v1/extensions/update" && method === "POST") {
      return json(response, 200, {
        offered: piLock.map((held) =>
          offer("update", held.name, {
            from: held.version,
            to: "0.4.0",
            changelog: "## 0.4.0\n\n- Notes survive a restart.\n",
          }),
        ),
        up_to_date: [],
        failed: [{ package: "pi-broken", error: "the registry answered 503" }],
      });
    }
    return false;
  }

  const fake: PackagesFake = {
    calls,
    problem: null,
    refuseNext(method, path, status, message) {
      refusals.set(`${method} ${path}`, { status, message });
    },
    waitForUpdate(name, from, to) {
      pagesA.addInboxItem({
        kind: "extension_update",
        title: `${name} ${from} -> ${to}`,
        data: { package: name, from, to, source: `npm:${name}` },
      });
      send({ kind: "extension_update_available", data: { name, version: to } });
    },
    installedNames: () => lock.map((each) => each.name),
    async handle(request, response, path) {
      if (!/^\/v1\/(packages|extensions)(\/|$)/.test(path)) return false;
      const method = request.method ?? "GET";
      const url = new URL(request.url ?? "/", "http://x");
      const body = method === "GET" ? null : await readBody(request);
      calls.push({ method, path: path + url.search, body });
      const refusal = refusals.get(`${method} ${path}`);
      if (refusal) {
        refusals.delete(`${method} ${path}`);
        return json(response, refusal.status, refusal.message);
      }
      return route(method, path, url.searchParams, (body ?? {}) as Obj, response);
    },
  };
  return fake;
}
