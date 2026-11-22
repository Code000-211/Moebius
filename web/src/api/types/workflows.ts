/** A workflow as its file writes it: see `docs/reference/workflows.md`. */
export interface WorkflowDefinition {
  name: string;
  description: string;
  trigger: Trigger[];
  nodes: WorkflowNode[];
  edges: WorkflowEdge[];
}

export interface Trigger {
  kind: string;
  params?: Record<string, unknown>;
}

export interface WorkflowNode {
  id: string;
  kind: string;
  params?: Record<string, unknown>;
  position: { x: number; y: number };
}

export interface WorkflowEdge {
  from: string;
  to: string;
  port: string;
}

/** The newest run of a workflow, as its row in the list tells it. */
export interface LastRun {
  id: number;
  status: string;
  /** When it was queued, in seconds since the epoch. */
  created_at: number;
  started_at: number | null;
  finished_at: number | null;
}

/** One workflow in the list. */
export interface WorkflowSummary {
  /** The id of the project the workflow belongs to. */
  project: number;
  project_name: string;
  name: string;
  description: string;
  /** The kind of the first trigger: `manual`, `schedule`, `event`, `webhook` or another; empty with none. */
  trigger_kind: string;
  node_count: number;
  last_run: LastRun | null;
  /** On a `schedule` trigger: when it fires next, in Unix seconds; absent while it is off or never comes due. */
  next_firing?: number;
  /** On a `schedule` trigger: the occurrence it last fired at; absent before it first fires. */
  last_fired?: number;
  /** On a `webhook` trigger: the address it answers on, with the project in it. */
  hook_address?: string;
}

/** One workflow's file and its parsed form. */
export interface WorkflowView {
  project: number;
  project_name: string;
  name: string;
  file: string;
  definition: WorkflowDefinition;
}

/** A saved workflow and the commit it made; `commit` is absent when the definition changed nothing. */
export interface SavedWorkflow extends WorkflowView {
  commit?: string;
}

/** A deleted workflow and the commit it made. */
export interface DeletedWorkflow {
  project: number;
  name: string;
  commit?: string;
}

/** One run of a workflow, as the list of runs and the run's page tell it. */
export interface WorkflowRunSummary {
  id: number;
  workflow: string;
  repository_id: number;
  /** The kind of the trigger that started it. */
  trigger: string;
  /** `queued`, `running`, `waiting`, `succeeded`, `failed` or `cancelled`. */
  status: string;
  /** The value the run was started with. */
  value: unknown;
  created_at: number;
  started_at: number | null;
  finished_at: number | null;
  error: string | null;
  /** The project of the workflow; null for a run from before workflows belonged to projects. */
  project: number | null;
  /** Null when the run has no project or the project is gone. */
  project_name: string | null;
}

/** One node of one run: what it was given, what it answered and why it failed. */
export interface WorkflowNodeRun {
  /** The id the node has in the definition. */
  node: string;
  /** What the node does; empty when the workflow no longer has the node. */
  kind: string;
  attempt: number;
  /** `queued`, `running`, `waiting`, `succeeded`, `failed` or `skipped`. */
  status: string;
  input: unknown;
  output: unknown;
  error: string | null;
  started_at: number | null;
  finished_at: number | null;
}

/** A run that stopped on a node and asked for a person. */
export interface WorkflowWaiting {
  node: string;
  /** What the node was given, which is what it asks about. */
  asks: unknown;
}

/** One run with a panel's worth of data for each node. */
export interface WorkflowRun extends WorkflowRunSummary {
  nodes: WorkflowNodeRun[];
  /** Where and on what it waits, when its status is `waiting`; null otherwise. */
  waiting: WorkflowWaiting | null;
}

/** What `POST /v1/workflows/{name}/runs` answers: the run as stored. */
export interface StartedRun {
  id: number;
  workflow: string;
  repository: string;
  trigger: string;
  status: string;
  value: unknown;
  project: number | null;
  project_name: string | null;
}

/** One input of a manual trigger, as `GET /v1/workflows/{name}/run-inputs` lists it. */
export interface RunInput {
  name: string;
  /** `text`, `number`, `boolean`, `choice` or `repository`. */
  type: string;
  required: boolean;
  default?: unknown;
  choices?: string[];
}
