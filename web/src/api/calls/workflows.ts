/** The calls of the Workflows pages and the canvas: list, read, save, delete, run and cancel workflows, and read their runs. */
import { del, get, post, put } from "../http";
import type {
  DeletedWorkflow,
  RunInput,
  SavedWorkflow,
  StartedRun,
  WorkflowDefinition,
  WorkflowRun,
  WorkflowRunSummary,
  WorkflowSummary,
  WorkflowView,
} from "../types/workflows";

export function workflows(): Promise<WorkflowSummary[]> {
  return get<WorkflowSummary[]>("/v1/workflows");
}

export function workflow(name: string): Promise<WorkflowView> {
  return get<WorkflowView>(`/v1/workflows/${encodeURIComponent(name)}`);
}

/** Saves a canvas's parsed form; the server validates it and keeps every node's position. */
export function saveWorkflow(name: string, definition: WorkflowDefinition): Promise<SavedWorkflow> {
  return put<SavedWorkflow>(`/v1/workflows/${encodeURIComponent(name)}`, { definition });
}

/** Saves a whole TOML file. */
export function saveWorkflowFile(name: string, file: string): Promise<SavedWorkflow> {
  return put<SavedWorkflow>(`/v1/workflows/${encodeURIComponent(name)}`, { file });
}

/** Deletes a workflow; a run of it queued or running refuses the deletion. */
export function deleteWorkflow(name: string): Promise<DeletedWorkflow> {
  return del<DeletedWorkflow>(`/v1/workflows/${encodeURIComponent(name)}`);
}

/** The runs of one workflow, newest first. */
export function workflowRuns(workflow: string): Promise<WorkflowRunSummary[]> {
  return get<WorkflowRunSummary[]>(`/v1/workflow-runs?workflow=${encodeURIComponent(workflow)}`);
}

/** One run with its nodes. */
export function workflowRun(id: number): Promise<WorkflowRun> {
  return get<WorkflowRun>(`/v1/workflow-runs/${id}`);
}

/** The inputs the manual trigger of a workflow names; a conflict when its trigger is not manual. */
export function runInputs(name: string): Promise<{ trigger_kind: string; inputs: RunInput[] }> {
  return get(`/v1/workflows/${encodeURIComponent(name)}/run-inputs`);
}

/** Starts a run of a workflow by hand, with the inputs a person gave. */
export function startWorkflowRun(
  name: string,
  body: { repository?: string; inputs: Record<string, unknown> },
): Promise<StartedRun> {
  return post<StartedRun>(`/v1/workflows/${encodeURIComponent(name)}/runs`, body);
}

/** Cancels a run that has not ended. */
export function cancelWorkflowRun(id: number): Promise<WorkflowRunSummary> {
  return post<WorkflowRunSummary>(`/v1/workflows/runs/${id}/cancel`, {});
}
