/**
 * The calls of the project's Workflows tab and the canvas: a workflow is addressed as (project, name), so every call
 * of a definition names its project. A run is addressed by its id.
 */
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

/** One run with its nodes. */
export function workflowRun(id: number): Promise<WorkflowRun> {
  return get<WorkflowRun>(`/v1/workflow-runs/${id}`);
}

/** Cancels a run that has not ended. */
export function cancelWorkflowRun(id: number): Promise<WorkflowRunSummary> {
  return post<WorkflowRunSummary>(`/v1/workflows/runs/${id}/cancel`, {});
}

const inProject = (project: number, name: string): string =>
  `/v1/projects/${project}/workflows/${encodeURIComponent(name)}`;

/** The workflows of one project. */
export function projectWorkflows(project: number): Promise<WorkflowSummary[]> {
  return get<WorkflowSummary[]>(`/v1/projects/${project}/workflows`);
}

export function projectWorkflow(project: number, name: string): Promise<WorkflowView> {
  return get<WorkflowView>(inProject(project, name));
}

/** Saves a canvas's parsed form into a project's workflow. */
export function saveProjectWorkflow(
  project: number,
  name: string,
  definition: WorkflowDefinition,
): Promise<SavedWorkflow> {
  return put<SavedWorkflow>(inProject(project, name), { definition });
}

/** Saves a whole TOML file into a project's workflow. */
export function saveProjectWorkflowFile(
  project: number,
  name: string,
  file: string,
): Promise<SavedWorkflow> {
  return put<SavedWorkflow>(inProject(project, name), { file });
}

export function deleteProjectWorkflow(project: number, name: string): Promise<DeletedWorkflow> {
  return del<DeletedWorkflow>(inProject(project, name));
}

export function projectRunInputs(
  project: number,
  name: string,
): Promise<{ trigger_kind: string; inputs: RunInput[] }> {
  return get(`${inProject(project, name)}/run-inputs`);
}

/** Starts a run of a project's workflow; the repository must be one of the project's. */
export function startProjectWorkflowRun(
  project: number,
  name: string,
  body: { repository?: string; inputs: Record<string, unknown> },
): Promise<StartedRun> {
  return post<StartedRun>(`${inProject(project, name)}/runs`, body);
}

/** The runs of a project, newest first, of one workflow when `workflow` is given. */
export function projectWorkflowRuns(
  project: number,
  workflow?: string,
): Promise<WorkflowRunSummary[]> {
  const query = new URLSearchParams({ project: String(project) });
  if (workflow !== undefined) query.set("workflow", workflow);
  return get<WorkflowRunSummary[]>(`/v1/workflow-runs?${query}`);
}
