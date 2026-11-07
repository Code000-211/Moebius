/**
 * The workflows the Workflows pages read, and the runs they show: two workflows whose graphs are saved, one run that
 * failed on a node and one that waits for a person. The shapes are the types of `src/api/workflows.ts`; the real server
 * answers the same routes once its workflow API lands. A test can change a run and broadcast the event the server would
 * send, which is how the live updates are tried, or empty the list.
 */
import type { IncomingMessage, ServerResponse } from "node:http";
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
  /** Each workflow's definition, by name. */
  readonly definitions: Map<string, WorkflowDefinition>;
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
            status: "done",
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
            status: "done",
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
  const workflows: WorkflowSummary[] = [
    summaryOf(release, lastFailed),
    summaryOf(deploy, { id: 42, status: "waiting", started_at: NOW() - 300, finished_at: null }),
  ];

  async function handle(
    request: IncomingMessage,
    response: ServerResponse,
    path: string,
  ): Promise<boolean> {
    if (request.method !== "GET") return false;
    const url = decoded(path);
    if (url === "/v1/workflows") return reply(response, 200, workflows);
    const run = /^\/v1\/workflow-runs\/(\d+)$/.exec(url);
    if (run) {
      const found = runs.get(Number(run[1]));
      if (!found) return reply(response, 404, "No such workflow run.");
      return reply(response, 200, found);
    }
    const definition = /^\/v1\/workflows\/([^/]+)$/.exec(url);
    if (definition) {
      const name = definition[1];
      const found = name === undefined ? undefined : definitions.get(name);
      if (!found) return reply(response, 404, "No such workflow.");
      return reply(response, 200, found);
    }
    return false;
  }

  return { handle, workflows, definitions, runs };
}
