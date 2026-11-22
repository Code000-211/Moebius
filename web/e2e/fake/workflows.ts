/**
 * The workflows the Workflows pages read, the runs they show and the routes they ask: three workflows whose graphs are
 * saved — one that failed on a node, one that waits for a person and one shaped like an automation (a schedule that
 * starts one chat) — and two runs. It answers what the real server answers, key for key (`web/src/api/fixtures/
 * workflows.json` holds that shape, and a test on each side holds the two to it): the list is made from the graphs and
 * the runs, so a run that is started or cancelled changes the list's last run, and a saved graph is the definition
 * the next read answers. A test can change a run and broadcast the event the server would send, which is how the live
 * updates are tried.
 *
 * A save is checked the way `WorkflowDefinition::validate` in the server checks it: in the same order and with the
 * same messages for the kinds the server's registry knows. A kind that only exists in these fixtures is taken as
 * known.
 */
import type { IncomingMessage, ServerResponse } from "node:http";
import type {
  WorkflowDefinition as GraphDefinition,
  Trigger as GraphTrigger,
  WorkflowRun,
  WorkflowRunSummary,
  WorkflowSummary,
} from "../../src/api/types/workflows";

const NOW = () => Math.floor(Date.now() / 1000);

export interface WorkflowsFake {
  /** Answers a route of the workflows and returns true; false for any other path. */
  handle(request: IncomingMessage, response: ServerResponse, path: string): Promise<boolean>;
  /** What `GET /v1/projects/1/workflows` answers now, in name order: each row is made from the graph and the runs. */
  readonly workflows: WorkflowSummary[];
  /** Each workflow's definition, by name, as the server answers it and the canvas saves it. A save replaces it. */
  readonly graphs: Map<string, GraphDefinition>;
  /** Each run, by id. A test changes one to stand in for the engine moving it on. */
  readonly runs: Map<number, WorkflowRun>;
  /** The requests of the workflows' routes, in order, so a test can read what a page sent. */
  readonly calls: Array<{ method: string | undefined; path: string }>;
  /** Makes the next `DELETE /v1/projects/1/workflows/{name}` answer `status` with `message`, as the server refuses it. */
  refuseNextDelete(status: number, message: string): void;
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

const LONG_OUTPUT = `${"compiling the workspace and linking every test binary\n".repeat(24)}done`;

/** The one project the fake's workflows belong to: `demo`, which the fake's projects list names too. */
const PROJECT = 1;
const PROJECT_NAME = "demo";
/** The repositories of that project, by name; a run is started in one of them. */
const PROJECT_REPOSITORIES = new Map([["demo", 1]]);

/** What the automations of the fake are: each is a schedule workflow of its project, `automation-<id>`. */
export type AutomationSource = () => Array<{ id: number; name: string; project_id: number }>;

/** What `GET /v1/workflows` answers for a workflow: its row, with the newest of its runs. */
function summaryOf(graph: GraphDefinition, runs: Iterable<WorkflowRun>): WorkflowSummary {
  const newest = [...runs]
    .filter((run) => run.workflow === graph.name)
    .sort((a, b) => b.id - a.id)[0];
  return {
    project: PROJECT,
    project_name: PROJECT_NAME,
    name: graph.name,
    description: graph.description,
    trigger_kind: graph.trigger[0]?.kind ?? "",
    node_count: graph.nodes.length,
    last_run: newest
      ? {
          id: newest.id,
          status: newest.status,
          created_at: newest.created_at,
          started_at: newest.started_at,
          finished_at: newest.finished_at,
        }
      : null,
  };
}

/** A run as the list of runs answers it: the run without its nodes. */
function briefOf(run: WorkflowRun): WorkflowRunSummary {
  const { nodes: _nodes, waiting: _waiting, ...brief } = run;
  return brief;
}

export function createWorkflowsFake(automations: AutomationSource = () => []): WorkflowsFake {
  const started = NOW() - 5 * 3600;
  const graphs = new Map<string, GraphDefinition>([
    [
      "release",
      {
        name: "release",
        description: "Build, test and publish a release.",
        trigger: [{ kind: "schedule", params: { cron: "0 6 * * 1" } }],
        nodes: [
          {
            id: "build",
            kind: "agent",
            params: { prompt: "Build the release", model: "auto" },
            position: { x: 16, y: 16 },
          },
          {
            id: "test",
            kind: "script",
            params: { command: "cargo test --workspace" },
            position: { x: 320, y: 16 },
          },
          {
            id: "publish",
            kind: "script",
            params: { command: "cargo publish" },
            position: { x: 624, y: 16 },
          },
        ],
        edges: [
          { from: "trigger", to: "build", port: "out" },
          { from: "build", to: "test", port: "out" },
          { from: "test", to: "publish", port: "out" },
        ],
      },
    ],
    [
      "deploy",
      {
        name: "deploy",
        description: "Build and deploy after a person approves.",
        trigger: [{ kind: "manual", params: {} }],
        nodes: [
          {
            id: "build",
            kind: "script",
            params: { command: "pnpm build" },
            position: { x: 16, y: 16 },
          },
          {
            id: "approve",
            kind: "wait",
            params: { question: "Publish 1.4.0 to the registry?" },
            position: { x: 320, y: 16 },
          },
        ],
        edges: [
          { from: "trigger", to: "build", port: "out" },
          { from: "build", to: "approve", port: "out" },
        ],
      },
    ],
    [
      "tidy",
      {
        name: "tidy",
        description: "Tidy old branches, every weeknight.",
        trigger: [{ kind: "schedule", params: { cron: "0 23 * * 1-5" } }],
        nodes: [
          {
            id: "chat",
            kind: "chat",
            params: { prompt: "Tidy the merged branches" },
            position: { x: 16, y: 16 },
          },
        ],
        edges: [{ from: "trigger", to: "chat", port: "out" }],
      },
    ],
  ]);
  const runs = new Map<number, WorkflowRun>([
    [
      41,
      {
        id: 41,
        project: PROJECT,
        project_name: PROJECT_NAME,
        workflow: "release",
        repository_id: 1,
        trigger: "schedule",
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
        project: PROJECT,
        project_name: PROJECT_NAME,
        workflow: "deploy",
        repository_id: 1,
        trigger: "manual",
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
  /** The text of each saved workflow's file; what a save wrote, or what the graph renders as. */
  const files = new Map<string, string>([...graphs].map(([name, graph]) => [name, render(graph)]));
  /** What `GET /v1/workflows` answers, in name order and from the graphs and runs as they stand now. */
  const listed = (): WorkflowSummary[] => [
    ...[...graphs.values()]
      .sort((a, b) => a.name.localeCompare(b.name))
      .map((graph) => summaryOf(graph, runs.values())),
    // An automation is a schedule that starts one chat: the server keeps it as a workflow named like it.
    ...automations()
      .filter((each) => each.project_id === PROJECT)
      .map(
        (each): WorkflowSummary => ({
          project: PROJECT,
          project_name: PROJECT_NAME,
          name: `automation-${each.id}`,
          description: "A schedule that starts one chat.",
          trigger_kind: "schedule",
          node_count: 1,
          last_run: null,
        }),
      ),
  ];
  /** Each save, so a test can read what was written and the page can name the commit. */
  let saves = 0;
  const calls: Array<{ method: string | undefined; path: string }> = [];
  let nextDeleteRefusal: { status: number; message: string } | null = null;

  async function handle(
    request: IncomingMessage,
    response: ServerResponse,
    path: string,
  ): Promise<boolean> {
    // The fake has one project: a workflow is addressed in it, and the routes that name none are gone, as on the
    // server. Inside, the project's routes are read as `/v1/workflows...` for the rest of the handler.
    const asked = decoded(path);
    const projectRoute = /^\/v1\/projects\/(\d+)\/workflows(?=\/|$)/.exec(asked);
    if (projectRoute && Number(projectRoute[1]) !== PROJECT) {
      return plain(response, 404, `project ${projectRoute[1]} not found`);
    }
    const url = projectRoute ? asked.replace(projectRoute[0], "/v1/workflows") : asked;
    if (!projectRoute && /^\/v1\/workflows(?:\/(?!runs\/\d+\/cancel$)|$)/.test(asked)) {
      calls.push({ method: request.method, path: asked });
      return plain(response, 410, "workflows belong to a project: use /v1/projects/{id}/workflows");
    }
    const named = /^\/v1\/workflows\/([^/]+)$/.exec(url);
    calls.push({ method: request.method, path: asked });
    if (named) {
      const name = named[1] ?? "";
      const found = graphs.get(name);
      if (request.method === "DELETE") {
        const refusal = nextDeleteRefusal;
        nextDeleteRefusal = null;
        if (refusal) return plain(response, refusal.status, refusal.message);
        if (!found) return plain(response, 404, `workflow ${name} not found`);
        graphs.delete(name);
        files.delete(name);
        return reply(response, 200, {
          project: PROJECT,
          name,
          commit: `${(0x1a2b3c + saves).toString(16)}${"0".repeat(34)}${saves}`,
        });
      }
      if (request.method === "PUT") {
        return save(name, request, response);
      }
      if (request.method !== "GET") return false;
      if (!found) return plain(response, 404, `workflow ${name} not found`);
      return reply(response, 200, {
        project: PROJECT,
        project_name: PROJECT_NAME,
        name,
        file: files.get(name) ?? "",
        definition: found,
      });
    }
    const inputs = /^\/v1\/workflows\/([^/]+)\/run-inputs$/.exec(url);
    if (inputs && request.method === "GET") {
      const found = graphs.get(inputs[1] ?? "");
      if (!found) return plain(response, 404, `workflow ${inputs[1]} not found`);
      if (found.trigger[0]?.kind !== "manual") {
        return plain(
          response,
          409,
          `the workflow's trigger is ${found.trigger[0]?.kind}, not manual`,
        );
      }
      return reply(response, 200, { trigger_kind: "manual", inputs: [] });
    }
    const starting = /^\/v1\/workflows\/([^/]+)\/runs$/.exec(url);
    if (starting && request.method === "POST") {
      const found = graphs.get(starting[1] ?? "");
      if (!found) return plain(response, 404, `workflow ${starting[1]} not found`);
      const body = JSON.parse((await readBody(request)) || "{}") as { repository?: string };
      if (!body.repository) {
        return plain(
          response,
          422,
          'the trigger has no repository input, so "repository" names the run\'s repository',
        );
      }
      const repositoryId = PROJECT_REPOSITORIES.get(body.repository);
      if (repositoryId === undefined) {
        return plain(
          response,
          422,
          `repository ${body.repository} is not one of the repositories of project ${PROJECT_NAME}`,
        );
      }
      const id = Math.max(0, ...runs.keys()) + 1;
      const run: WorkflowRun = {
        id,
        project: PROJECT,
        project_name: PROJECT_NAME,
        workflow: found.name,
        repository_id: repositoryId,
        trigger: "manual",
        status: "running",
        value: {},
        created_at: NOW(),
        started_at: NOW(),
        finished_at: null,
        error: null,
        nodes: [],
        waiting: null,
      };
      runs.set(id, run);
      return reply(response, 200, {
        id,
        workflow: found.name,
        repository: body.repository,
        trigger: "manual",
        status: "running",
        value: {},
        project: PROJECT,
        project_name: PROJECT_NAME,
      });
    }
    const cancelling = /^\/v1\/workflows\/runs\/(\d+)\/cancel$/.exec(url);
    if (cancelling && request.method === "POST") {
      const run = runs.get(Number(cancelling[1]));
      if (!run) return plain(response, 404, `workflow run ${cancelling[1]} not found`);
      if (["succeeded", "failed", "cancelled"].includes(run.status)) {
        return plain(response, 409, `workflow run ${run.id} has already ended as ${run.status}`);
      }
      run.status = "cancelled";
      run.error = "cancelled by a person";
      run.finished_at = NOW();
      return reply(response, 200, briefOf(run));
    }
    if (request.method !== "GET") return false;
    if (url === "/v1/workflows") return reply(response, 200, listed());
    const list = /^\/v1\/workflow-runs\?(.*)$/.exec(decoded(request.url ?? url));
    if (list) {
      const query = new URLSearchParams(list[1]);
      const wanted = query.get("workflow");
      const project = query.get("project");
      const ofWorkflow = [...runs.values()]
        .filter((entry) => project === null || Number(project) === entry.project)
        .filter((entry) => wanted === null || entry.workflow === wanted)
        .sort((a, b) => b.id - a.id)
        .map(briefOf);
      return reply(response, 200, ofWorkflow);
    }
    const run = /^\/v1\/workflow-runs\/(\d+)$/.exec(url);
    if (run) {
      const found = runs.get(Number(run[1]));
      if (!found) return plain(response, 404, `workflow run ${run[1]} not found`);
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
    files.set(name, render(saved));
    saves += 1;
    return reply(response, 200, {
      project: PROJECT,
      project_name: PROJECT_NAME,
      name,
      file: render(saved),
      definition: saved,
      commit: `${(0x1a2b3c + saves * 0x1000).toString(16)}${"0".repeat(34)}${saves}`,
    });
  }

  return {
    handle,
    get workflows() {
      return listed();
    },
    graphs,
    runs,
    calls,
    refuseNextDelete(status: number, message: string) {
      nextDeleteRefusal = { status, message };
    },
  };
}

async function readBody(request: IncomingMessage): Promise<string> {
  const chunks: Buffer[] = [];
  for await (const chunk of request) {
    chunks.push(chunk as Buffer);
  }
  return Buffer.concat(chunks).toString("utf8");
}
