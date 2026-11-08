/** The calls of the workflow canvas and the command line: list, read, save and delete workflows. */
import { del, get, put } from "../http";
import type {
  DeletedWorkflow,
  SavedWorkflow,
  WorkflowDefinition,
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
