/** The routes of the app's area: workflows: a definition, its canvas, a run of one, and the automations. Pages are fetched when their route opens. */
import { lazy } from "react";

const AutomationsPage = lazy(() =>
  import("../automations/AutomationsPage").then((all) => ({ default: all.AutomationsPage })),
);
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
  { path: "automations", element: <AutomationsPage /> },
  { path: "workflows/:name", element: <WorkflowPage /> },
  { path: "workflows/:name/canvas", element: <WorkflowCanvasPage /> },
  { path: "workflow-runs/:id", element: <WorkflowRunPage /> },
];
