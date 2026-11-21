/** The routes of the app's area: workspace: the issues, views, milestones, cycles, intake and the wiki. Pages are fetched when their route opens. */
import { lazy } from "react";

const IssuesPage = lazy(() =>
  import("../issues/IssuesPage").then((all) => ({ default: all.IssuesPage })),
);
const MyIssuesPage = lazy(() =>
  import("../issues/MyIssuesPage").then((all) => ({ default: all.MyIssuesPage })),
);
const ViewsPage = lazy(() =>
  import("../views/ViewsPage").then((all) => ({ default: all.ViewsPage })),
);
const MilestonesPage = lazy(() =>
  import("../milestones/MilestonesPage").then((all) => ({ default: all.MilestonesPage })),
);
const CyclesPage = lazy(() =>
  import("../cycles/CyclesPage").then((all) => ({ default: all.CyclesPage })),
);
const IntakePage = lazy(() =>
  import("../intake/IntakePage").then((all) => ({ default: all.IntakePage })),
);
const WikiPage = lazy(() => import("../wiki/WikiPage").then((all) => ({ default: all.WikiPage })));

/** The routes of this area, as children of the shell's route. */
export const workspaceRoutes = [
  { path: "issues", element: <IssuesPage /> },
  { path: "my-issues", element: <MyIssuesPage /> },
  { path: "views", element: <ViewsPage /> },
  { path: "views/:id", element: <ViewsPage /> },
  { path: "milestones", element: <MilestonesPage /> },
  { path: "cycles", element: <CyclesPage /> },
  { path: "intake", element: <IntakePage /> },
  { path: "wiki", element: <WikiPage /> },
];
