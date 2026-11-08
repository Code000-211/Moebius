/**
 * The calls and answers of the Workflows pages: the workflows the harness configuration holds, one workflow's saved
 * definition, and one run with the input, output, log and error of each of its nodes.
 *
 * The routes are `GET /v1/workflows`, `GET /v1/workflows/{name}` and `GET /v1/workflow-runs/{id}`. The server's
 * workflow API is built in its own cards; until it lands, `e2e/fake/workflows.ts` answers these shapes, and this file
 * is the one place a page reads them from.
 */
import { get } from "./http";

/** What starts a workflow: its kind, and the schedule, event or address that goes with it. */
export interface WorkflowTrigger {
  /** `manual`, `schedule`, `event` or `webhook`. */
  kind: string;
  /** The cron expression, event name or address, when the kind has one. */
  detail: string | null;
}

/** The last run of a workflow, as its list row tells it. */
export interface WorkflowRunSummary {
  id: number;
  status: string;
  /** When it was queued, in seconds since the epoch. */
  started_at: number | null;
  finished_at: number | null;
}

/** One workflow of the list, without its graph or its file. */
export interface WorkflowSummary {
  name: string;
  description: string;
  trigger: WorkflowTrigger;
  node_count: number;
  last_run: WorkflowRunSummary | null;
}

/** A node saved on the definition's canvas, at the place a person put it. */
export interface WorkflowNode {
  id: string;
  /** What the node does: `agent`, `script`, `check`, `wait`, `merge` and the other kinds of the engine. */
  kind: string;
  /** The values the node is given, as the file saves them. */
  parameters: unknown;
  /** Where the canvas saved it, in pixels from the top left of the graph. */
  position: { x: number; y: number };
}

/** A line of the graph, from one node to the node that waits for it. */
export interface WorkflowEdge {
  from: string;
  to: string;
}

/** One workflow with its graph and the file it was saved in. */
export interface WorkflowDefinition extends WorkflowSummary {
  nodes: WorkflowNode[];
  edges: WorkflowEdge[];
  /** The workflow's file, exactly as it is stored, for copying. */
  file: string;
}

/** One node of one run: what it was given, what it answered, what it logged and why it failed. */
export interface WorkflowNodeRun {
  /** The id the node has in the definition. */
  node: string;
  kind: string;
  attempt: number;
  /** `queued`, `running`, `waiting`, `succeeded`, `failed` or `skipped`. */
  status: string;
  input: unknown;
  output: unknown;
  /** What the node printed, when it printed anything. */
  log: string | null;
  error: string | null;
  started_at: number | null;
  finished_at: number | null;
}

/** A run that stopped on a node and asked for a person. */
export interface WorkflowWaiting {
  node: string;
  /** What the run asks, as the node handed it over. */
  asks: unknown;
}

/** One run of a workflow, with a panel's worth of data for each node. */
export interface WorkflowRun {
  id: number;
  workflow: string;
  repository_id: number;
  trigger: WorkflowTrigger;
  /** `queued`, `running`, `waiting`, `succeeded`, `failed` or `cancelled`. */
  status: string;
  /** The value the run was started with. */
  value: unknown;
  created_at: number;
  started_at: number | null;
  finished_at: number | null;
  error: string | null;
  nodes: WorkflowNodeRun[];
  /** Where and what it waits for, when its status is `waiting`; null otherwise. */
  waiting: WorkflowWaiting | null;
}

/** Every workflow the server knows, in the order its list shows them. */
export function workflows(): Promise<WorkflowSummary[]> {
  return get<WorkflowSummary[]>("/v1/workflows");
}

/** One workflow's saved definition and file. */
export function workflow(name: string): Promise<WorkflowDefinition> {
  return get<WorkflowDefinition>(`/v1/workflows/${encodeURIComponent(name)}`);
}

/** One run with its nodes. */
export function workflowRun(id: number): Promise<WorkflowRun> {
  return get<WorkflowRun>(`/v1/workflow-runs/${id}`);
}
