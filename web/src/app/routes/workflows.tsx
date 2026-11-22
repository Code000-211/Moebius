/** The routes of the app's area: workflows: the definition of one, its canvas, and a run of one; the list and the runs are the Workflows tab of a project. Pages are fetched when their route opens. */
import { lazy } from "react";

const WorkflowPage = lazy(() =>
  import("../workflows/WorkflowPage").then((all) => ({ default: all.WorkflowPage })),
);
const WorkflowCanvasPage = lazy(() =>
  import("../workflows/canvas/WorkflowCanvasPage").then((all) => ({
    default: all.WorkflowCanvasPage,
  })),
);
const WorkflowRunPage = lazy(() =>
  import("../workflows/WorkflowRunPage").then((all) => ({ default: all.WorkflowRunPage })),
);

/** The routes of this area, as children of the shell's route. */
export const workflowsRoutes = [
  { path: "projects/:projectId/workflows/:name", element: <WorkflowPage /> },
  { path: "projects/:projectId/workflows/:name/canvas", element: <WorkflowCanvasPage /> },
  { path: "workflow-runs/:id", element: <WorkflowRunPage /> },
];
