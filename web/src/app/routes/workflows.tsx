/** The routes of the app's area: workflows: one workflow's page (Overview, Canvas, Runs and File tabs) and a run of one; the list and the runs are the Workflows tab of a project. Pages are fetched when their route opens. */
import { lazy } from "react";

const WorkflowPage = lazy(() =>
  import("../workflows/WorkflowPage").then((all) => ({ default: all.WorkflowPage })),
);
const WorkflowRunPage = lazy(() =>
  import("../workflows/WorkflowRunPage").then((all) => ({ default: all.WorkflowRunPage })),
);

/** The routes of this area, as children of the shell's route. */
export const workflowsRoutes = [
  // One page per workflow; the tab is the last segment: Overview (none), `canvas`, `runs` or `file`.
  { path: "projects/:projectId/workflows/:name/:tab?", element: <WorkflowPage /> },
  { path: "workflow-runs/:id", element: <WorkflowRunPage /> },
];
