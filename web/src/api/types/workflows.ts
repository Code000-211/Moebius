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

/** One workflow in the list. */
export interface WorkflowSummary {
  name: string;
  description: string;
  trigger_kind: string;
  node_count: number;
}

/** One workflow's file and its parsed form. */
export interface WorkflowView {
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
  name: string;
  commit?: string;
}
