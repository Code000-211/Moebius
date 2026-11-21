/** The routes of the app's area: workspace: the issues, views and the wiki (cycles, milestones and intake are tabs of a project). Pages are fetched when their route opens. */
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
const WikiPage = lazy(() => import("../wiki/WikiPage").then((all) => ({ default: all.WikiPage })));

/** The routes of this area, as children of the shell's route. */
export const workspaceRoutes = [
  { path: "issues", element: <IssuesPage /> },
  { path: "my-issues", element: <MyIssuesPage /> },
  { path: "views", element: <ViewsPage /> },
  { path: "views/:id", element: <ViewsPage /> },
  { path: "wiki", element: <WikiPage /> },
];
