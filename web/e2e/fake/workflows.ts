/**
 * The workflows the Workflows pages read, and the runs they show: two workflows whose graphs are saved, one run that
 * failed on a node and one that waits for a person. The shapes are the types of `src/api/workflows.ts`; the real server
 * answers the same routes once its workflow API lands. A test can change a run and broadcast the event the server would
 * send, which is how the live updates are tried, or empty the list.
 *
 * A workflow answers with both shapes at once: the flat one the read-only definition page reads, and the parsed graph
 * under `definition` that the canvas edits and that `PUT /v1/workflows/{name}` takes back. A save is checked the way
 * `WorkflowDefinition::validate` in the server checks it: in the same order and with the same messages for the kinds
 * the server's registry knows. A kind that only exists in these fixtures is taken as known, because the registry the
 * real server runs is still being filled by its own cards.
 */
import type { IncomingMessage, ServerResponse } from "node:http";
import type {
  WorkflowDefinition as GraphDefinition,
  Trigger as GraphTrigger,
} from "../../src/api/types/workflows";
import type {
  WorkflowDefinition,
  WorkflowRun,
  WorkflowRunSummary,
  WorkflowSummary,
} from "../../src/api/workflows";

const NOW = () => Math.floor(Date.now() / 1000);

export interface WorkflowsFake {
  /** Answers a route of the workflows and returns true; false for any other path. */
  handle(request: IncomingMessage, response: ServerResponse, path: string): Promise<boolean>;
  /** What `GET /v1/workflows` answers, in order. A test that needs an empty page empties it. */
  readonly workflows: WorkflowSummary[];
  /** Each workflow's definition, by name, as the read-only pages read it. */
  readonly definitions: Map<string, WorkflowDefinition>;
  /** Each workflow's graph, by name, as the canvas saves it. A save replaces it. */
  readonly graphs: Map<string, GraphDefinition>;
  /** Each run, by id. A test changes one to stand in for the engine moving it on. */
  readonly runs: Map<number, WorkflowRun>;
}

function reply(response: ServerResponse, status: number, body: unknown): true {
  response.writeHead(status, { "content-type": "application/json" });
  response.end(JSON.stringify(body));
  return true;
}

/** The path as a person writes it: a segment with a space arrives from the browser as `%20`. */
function decoded(path: string): string {
  try {
    return decodeURIComponent(path);
  } catch {
    return path;
  }
}

function plain(response: ServerResponse, status: number, body: string): true {
  response.writeHead(status, { "content-type": "text/plain" });
  response.end(body);
  return true;
}

/**
 * Why a graph would be refused, in the order the server checks: one trigger, the ids, edges that name nodes, kinds
 * and ports, a `for_each` sub-path that rejoins where it says, no cycle, and reaching every node from the trigger.
 * `null` for a graph that would be written.
 */
export function refuse(graph: GraphDefinition): string | null {
  if (graph.trigger.length !== 1) {
    const found = graph.trigger.length === 0 ? "none" : String(graph.trigger.length);
    return `exactly one trigger is required, found ${found}`;
  }
  const ids = new Set<string>();
  for (const node of graph.nodes) {
    if (node.id === "trigger") {
      return 'node "trigger": the id is reserved for the workflow\'s trigger';
    }
    if (ids.has(node.id)) {
      return `node ${quote(node.id)}: duplicate id`;
    }
    ids.add(node.id);
  }
  for (const edge of graph.edges) {
    if (edge.from !== "trigger" && !ids.has(edge.from)) {
      return `edge ${quote(edge.from)} -> ${quote(edge.to)}: no node ${quote(edge.from)}`;
    }
    if (!ids.has(edge.to)) {
      return `edge ${quote(edge.from)} -> ${quote(edge.to)}: no node ${quote(edge.to)}`;
    }
  }

  const trigger = graph.trigger[0];
  if (!trigger) {
    return "exactly one trigger is required, found none";
  }
  const triggerNode = knownNode(trigger.kind, asRecord(trigger.params));
  if (triggerNode && "error" in triggerNode) {
    return `trigger: ${triggerNode.error}`;
  }
  const nodes = new Map<string, KnownNode | null>();
  for (const node of graph.nodes) {
    const known = knownNode(node.kind, asRecord(node.params));
    if (known && "error" in known) {
      return `node ${quote(node.id)}: ${known.error}`;
    }
    nodes.set(node.id, known);
  }
  for (const edge of graph.edges) {
    const source = edge.from === "trigger" ? triggerNode : (nodes.get(edge.from) ?? null);
    if (source && "outputs" in source && !source.outputs.includes(edge.port)) {
      return `node ${quote(edge.from)}: edge to ${quote(edge.to)} leaves by port ${quote(edge.port)}, which is not one of its outputs`;
    }
  }
  for (const node of graph.nodes) {
    if (node.kind !== "condition") {
      continue;
    }
    const known = nodes.get(node.id);
    if (!known || !("outputs" in known)) {
      continue;
    }
    for (const port of known.outputs) {
      if (!graph.edges.some((edge) => edge.from === node.id && edge.port === port)) {
        return `node ${quote(node.id)}: port ${quote(port)} has no edge leaving by it`;
      }
    }
  }

  const adjacency = new Map<string, string[]>();
  for (const edge of graph.edges) {
    const targets = adjacency.get(edge.from) ?? [];
    targets.push(edge.to);
    adjacency.set(edge.from, targets);
  }
  for (const node of graph.nodes) {
    if (node.kind !== "for_each") {
      continue;
    }
    const known = knownNode(node.kind, asRecord(node.params));
    if (!known || "error" in known) {
      continue;
    }
    const rejoin = asRecord(node.params).rejoin as string;
    if (rejoin === node.id || !ids.has(rejoin)) {
      return `node ${quote(node.id)}: rejoin names ${quote(rejoin)}, which is not another node of the workflow`;
    }
    const starts = graph.edges
      .filter((edge) => edge.from === node.id && edge.port === "each")
      .map((edge) => edge.to);
    if (starts.length === 0) {
      return `node ${quote(node.id)}: no edge leaves by port "each", so there is no sub-path`;
    }
    const seen = new Set<string>();
    const queue = [...starts];
    while (queue.length > 0) {
      const id = queue.shift() ?? "";
      if (!seen.has(id)) {
        seen.add(id);
        queue.push(...(adjacency.get(id) ?? []));
      }
    }
    if (!seen.has(rejoin)) {
      return `node ${quote(node.id)}: the sub-path leaving "each" does not rejoin at ${quote(rejoin)}`;
    }
  }

  const visitState = new Map<string, "new" | "on-stack" | "done">();
  const stack: string[] = [];
  const visit = (id: string): string[] | null => {
    const state = visitState.get(id) ?? "new";
    if (state === "done") {
      return null;
    }
    if (state === "on-stack") {
      return [id];
    }
    visitState.set(id, "on-stack");
    stack.push(id);
    for (const target of adjacency.get(id) ?? []) {
      const seen = visitState.get(target) ?? "new";
      if (seen === "on-stack") {
        return stack.slice(stack.indexOf(target));
      }
      if (seen === "new") {
        const cycle = visit(target);
        if (cycle) {
          return cycle;
        }
      }
    }
    stack.pop();
    visitState.set(id, "done");
    return null;
  };
  for (const node of graph.nodes) {
    const cycle = visit(node.id);
    if (cycle) {
      return `node ${quote(cycle[0] ?? "")}: is part of a cycle through ${cycle.map(quote).join(" -> ")}`;
    }
  }

  const reached = new Set<string>();
  const ahead = [...(adjacency.get("trigger") ?? [])];
  while (ahead.length > 0) {
    const id = ahead.shift() ?? "";
    if (reached.has(id)) {
      continue;
    }
    reached.add(id);
    ahead.push(...(adjacency.get(id) ?? []));
  }
  for (const node of graph.nodes) {
    if (!reached.has(node.id)) {
      return `node ${quote(node.id)}: is unreachable from the trigger`;
    }
  }
  return null;
}

/** A string the way the server's `{:?}` writes it. */
function quote(value: string): string {
  return JSON.stringify(value);
}

/** Parameters as the graph holds them; the flat fixtures keep them as plain values. */
function asRecord(params: unknown): Record<string, unknown> {
  return typeof params === "object" && params !== null ? (params as Record<string, unknown>) : {};
}

/** A node the server's registry knows, or `null` for a kind that only exists in these fixtures. */
type KnownNode = { outputs: string[] } | { error: string };

/** The node a kind builds, or the parameter error the server gives for it. */
function knownNode(kind: string, params: Record<string, unknown>): KnownNode | null {
  if (kind === "code") {
    if (typeof params.source !== "string") {
      return {
        error: 'invalid parameters for node kind "code": "source" is required',
      };
    }
    return { outputs: ["out"] };
  }
  if (kind === "condition") {
    if (!Array.isArray(params.ports) || params.ports.length === 0) {
      return {
        error: 'invalid parameters for node kind "condition": at least one port is required',
      };
    }
    const outputs: string[] = [];
    for (const entry of params.ports) {
      const port = asRecord(entry);
      if (typeof port.name !== "string" || port.name.trim() === "") {
        return {
          error: 'invalid parameters for node kind "condition": a port needs a name',
        };
      }
      if (typeof port.when !== "string" || port.when.trim() === "") {
        return {
          error: `invalid parameters for node kind "condition": port ${quote(port.name)} needs an expression`,
        };
      }
      outputs.push(port.name);
    }
    if (typeof params.fallback === "string" && params.fallback !== "") {
      outputs.push(params.fallback);
    }
    return { outputs };
  }
  if (kind === "merge") {
    for (const side of ["a", "b"]) {
      if (typeof params[side] !== "string" || (params[side] as string).trim() === "") {
        return {
          error: `invalid parameters for node kind "merge": "${side}" must name the node whose value arrives by port "${side}"`,
        };
      }
    }
    if (params.a === params.b) {
      return {
        error: 'invalid parameters for node kind "merge": "a" and "b" must name different nodes',
      };
    }
    return { outputs: ["out"] };
  }
  if (kind !== "for_each") {
    return null;
  }
  if (typeof params.rejoin !== "string" || params.rejoin.length === 0) {
    return {
      error:
        'invalid parameters for node kind "for_each": "rejoin" must name the node where the sub-path rejoins',
    };
  }
  if (
    "max_elements" in params &&
    (typeof params.max_elements !== "number" ||
      !Number.isInteger(params.max_elements) ||
      params.max_elements < 0)
  ) {
    return {
      error:
        'invalid parameters for node kind "for_each": "max_elements" must be a non-negative integer',
    };
  }
  return { outputs: ["each", "done"] };
}

/** The graph as the file that holds it, so a save leaves the read-only page saying something true. */
function render(graph: GraphDefinition): string {
  const trigger = graph.trigger[0];
  const lines = [
    `name = ${JSON.stringify(graph.name)}`,
    `description = ${JSON.stringify(graph.description)}`,
    `trigger = ${inlineTrigger(trigger)}`,
    "",
  ];
  for (const node of graph.nodes) {
    lines.push(
      "[[nodes]]",
      `id = ${JSON.stringify(node.id)}`,
      `kind = ${JSON.stringify(node.kind)}`,
      `at = [${node.position.x}, ${node.position.y}]`,
      `[nodes.parameters]`,
      ...Object.entries(asRecord(node.params)).map(
        ([key, value]) => `${key} = ${JSON.stringify(value)}`,
      ),
      "",
    );
  }
  for (const edge of graph.edges) {
    lines.push(
      "[[edges]]",
      `from = ${JSON.stringify(edge.from)}`,
      `to = ${JSON.stringify(edge.to)}`,
      `port = ${JSON.stringify(edge.port)}`,
    );
  }
  return `${lines.join("\n").trimEnd()}\n`;
}

function inlineTrigger(trigger: GraphTrigger | undefined): string {
  if (!trigger) {
    return "{ }";
  }
  const parts = Object.entries(asRecord(trigger.params)).map(
    ([key, value]) => `${key} = ${JSON.stringify(value)}`,
  );
  return `{ kind = ${JSON.stringify(trigger.kind)}${parts.length > 0 ? `, ${parts.join(", ")}` : ""} }`;
}

const RELEASE_FILE = `name = "release"
description = "Build, test and publish a release"
trigger = { kind = "schedule", detail = "0 6 * * 1" }

[[nodes]]
id = "build"
kind = "agent"
at = [16, 16]
[nodes.parameters]
prompt = "Build the release"
model = "auto"

[[nodes]]
id = "test"
kind = "script"
at = [320, 16]
[nodes.parameters]
command = "cargo test --workspace"

[[nodes]]
id = "publish"
kind = "script"
at = [624, 16]
[nodes.parameters]
command = "cargo publish"
`;

const DEPLOY_FILE = `name = "deploy"
description = "Build and deploy after a person approves"
trigger = { kind = "manual" }

[[nodes]]
id = "build"
kind = "script"
at = [16, 16]
[nodes.parameters]
command = "pnpm build"

[[nodes]]
id = "approve"
kind = "wait"
at = [320, 16]
[nodes.parameters]
question = "Publish 1.4.0 to the registry?"
`;

const LONG_OUTPUT = `${"compiling the workspace and linking every test binary\n".repeat(24)}done`;

function summaryOf(
  definition: WorkflowDefinition,
  last: WorkflowRunSummary | null,
): WorkflowSummary {
  return {
    name: definition.name,
    description: definition.description,
    trigger: definition.trigger,
    node_count: definition.nodes.length,
    last_run: last,
  };
}

export function createWorkflowsFake(): WorkflowsFake {
  const started = NOW() - 5 * 3600;
  const lastFailed: WorkflowRunSummary = {
    id: 41,
    status: "failed",
    started_at: started,
    finished_at: started + 94,
  };
  const release: WorkflowDefinition = {
    name: "release",
    description: "Build, test and publish a release.",
    trigger: { kind: "schedule", detail: "0 6 * * 1" },
    node_count: 3,
    last_run: lastFailed,
    nodes: [
      {
        id: "build",
        kind: "agent",
        parameters: { prompt: "Build the release", model: "auto" },
        position: { x: 16, y: 16 },
      },
      {
        id: "test",
        kind: "script",
        parameters: { command: "cargo test --workspace" },
        position: { x: 320, y: 16 },
      },
      {
        id: "publish",
        kind: "script",
        parameters: { command: "cargo publish" },
        position: { x: 624, y: 16 },
      },
    ],
    edges: [
      { from: "build", to: "test" },
      { from: "test", to: "publish" },
    ],
    file: RELEASE_FILE,
  };
  const deploy: WorkflowDefinition = {
    name: "deploy",
    description: "Build and deploy after a person approves.",
    trigger: { kind: "manual", detail: null },
    node_count: 2,
    last_run: {
      id: 42,
      status: "waiting",
      started_at: NOW() - 300,
      finished_at: null,
    },
    nodes: [
      {
        id: "build",
        kind: "script",
        parameters: { command: "pnpm build" },
        position: { x: 16, y: 16 },
      },
      {
        id: "approve",
        kind: "wait",
        parameters: { question: "Publish 1.4.0 to the registry?" },
        position: { x: 320, y: 16 },
      },
    ],
    edges: [{ from: "build", to: "approve" }],
    file: DEPLOY_FILE,
  };
  const definitions = new Map<string, WorkflowDefinition>([
    [release.name, release],
    [deploy.name, deploy],
  ]);
  const runs = new Map<number, WorkflowRun>([
    [
      41,
      {
        id: 41,
        workflow: release.name,
        repository_id: 1,
        trigger: release.trigger,
        status: "failed",
        value: { tag: "v1.4.0" },
        created_at: started,
        started_at: started,
        finished_at: started + 94,
        error: "The test node failed.",
        nodes: [
          {
            node: "build",
            kind: "agent",
            attempt: 1,
            status: "succeeded",
            input: { prompt: "Build the release", model: "auto" },
            output: { artifact: "target/release/moebius", stdout: LONG_OUTPUT },
            log: "built the workspace in 41s",
            error: null,
            started_at: started,
            finished_at: started + 41,
          },
          {
            node: "test",
            kind: "script",
            attempt: 1,
            status: "failed",
            input: { command: "cargo test --workspace", token: "[redacted]" },
            output: null,
            log: "running 62 tests\n61 passed; 1 failed",
            error: "test first_card::a_card_moves failed: expected `done`, got `in progress`",
            started_at: started + 41,
            finished_at: started + 94,
          },
          {
            node: "publish",
            kind: "script",
            attempt: 1,
            status: "skipped",
            input: null,
            output: null,
            log: null,
            error: null,
            started_at: null,
            finished_at: null,
          },
        ],
        waiting: null,
      },
    ],
    [
      42,
      {
        id: 42,
        workflow: deploy.name,
        repository_id: 1,
        trigger: deploy.trigger,
        status: "waiting",
        value: { version: "1.4.0" },
        created_at: NOW() - 300,
        started_at: NOW() - 300,
        finished_at: null,
        error: null,
        nodes: [
          {
            node: "build",
            kind: "script",
            attempt: 1,
            status: "succeeded",
            input: { command: "pnpm build" },
            output: { files: 352 },
            log: "built in 9s",
            error: null,
            started_at: NOW() - 300,
            finished_at: NOW() - 291,
          },
          {
            node: "approve",
            kind: "wait",
            attempt: 1,
            status: "waiting",
            input: { version: "1.4.0", token: "[redacted]" },
            output: null,
            log: "waiting for a person",
            error: null,
            started_at: NOW() - 291,
            finished_at: null,
          },
        ],
        waiting: {
          node: "approve",
          asks: {
            question: "Publish 1.4.0 to the registry?",
            options: ["approve", "reject"],
            token: "[redacted]",
          },
        },
      },
    ],
  ]);
  const graphs = new Map<string, GraphDefinition>([
    [
      release.name,
      {
        name: release.name,
        description: release.description,
        trigger: [{ kind: "schedule", params: { detail: "0 6 * * 1" } }],
        nodes: release.nodes.map((node) => ({
          id: node.id,
          kind: node.kind,
          params: asRecord(node.parameters),
          position: { ...node.position },
        })),
        edges: [
          { from: "trigger", to: "build", port: "out" },
          ...release.edges.map((edge) => ({ from: edge.from, to: edge.to, port: "out" })),
        ],
      },
    ],
    [
      deploy.name,
      {
        name: deploy.name,
        description: deploy.description,
        trigger: [{ kind: "manual", params: {} }],
        nodes: deploy.nodes.map((node) => ({
          id: node.id,
          kind: node.kind,
          params: asRecord(node.parameters),
          position: { ...node.position },
        })),
        edges: [
          { from: "trigger", to: "build", port: "out" },
          ...deploy.edges.map((edge) => ({ from: edge.from, to: edge.to, port: "out" })),
        ],
      },
    ],
  ]);
  const workflows: WorkflowSummary[] = [
    summaryOf(release, lastFailed),
    summaryOf(deploy, { id: 42, status: "waiting", started_at: NOW() - 300, finished_at: null }),
  ];
  /** Each save, so a test can read what was written and the page can name the commit. */
  let saves = 0;

  async function handle(
    request: IncomingMessage,
    response: ServerResponse,
    path: string,
  ): Promise<boolean> {
    const url = decoded(path);
    const named = /^\/v1\/workflows\/([^/]+)$/.exec(url);
    if (named) {
      const name = named[1] ?? "";
      const found = definitions.get(name);
      if (!found) return plain(response, 404, "No such workflow.");
      if (request.method === "PUT") {
        return save(name, request, response);
      }
      if (request.method !== "GET") return false;
      // Both shapes: the flat one the read-only page reads, and the graph the canvas edits.
      return reply(response, 200, { ...found, definition: graphs.get(name) });
    }
    if (request.method !== "GET") return false;
    if (url === "/v1/workflows") return reply(response, 200, workflows);
    const list = /^\/v1\/workflow-runs\?(.*)$/.exec(decoded(request.url ?? url));
    if (list) {
      const wanted = new URLSearchParams(list[1]).get("workflow");
      const ofWorkflow = [...runs.values()]
        .filter((entry) => entry.workflow === wanted)
        .sort((a, b) => b.id - a.id)
        .map(
          (entry): WorkflowRunSummary => ({
            id: entry.id,
            status: entry.status,
            started_at: entry.started_at,
            finished_at: entry.finished_at,
          }),
        );
      return reply(response, 200, ofWorkflow);
    }
    const run = /^\/v1\/workflow-runs\/(\d+)$/.exec(url);
    if (run) {
      const found = runs.get(Number(run[1]));
      if (!found) return plain(response, 404, "No such workflow run.");
      return reply(response, 200, found);
    }
    return false;
  }

  /** `PUT /v1/workflows/{name}`: the graph is checked and, when it stands, written as a commit. */
  async function save(
    name: string,
    request: IncomingMessage,
    response: ServerResponse,
  ): Promise<boolean> {
    const raw = await readBody(request);
    let body: unknown;
    try {
      body = JSON.parse(raw);
    } catch {
      return plain(response, 400, "The body is not JSON.");
    }
    const graph = (body as { definition?: GraphDefinition }).definition;
    if (!graph) {
      return plain(response, 400, "The body has no definition.");
    }
    const saved: GraphDefinition = { ...graph, name };
    const refusal = refuse(saved);
    if (refusal !== null) {
      return plain(response, 422, `workflow workflows/${name}.toml: ${refusal}`);
    }
    graphs.set(name, saved);
    const flat = definitions.get(name);
    if (flat) {
      definitions.set(name, { ...flat, file: render(saved) });
    }
    saves += 1;
    return reply(response, 200, {
      name,
      file: render(saved),
      definition: saved,
      commit: `${(0x1a2b3c + saves * 0x1000).toString(16)}${"0".repeat(34)}${saves}`,
    });
  }

  return { handle, workflows, definitions, graphs, runs };
}

async function readBody(request: IncomingMessage): Promise<string> {
  const chunks: Buffer[] = [];
  for await (const chunk of request) {
    chunks.push(chunk as Buffer);
  }
  return Buffer.concat(chunks).toString("utf8");
}
