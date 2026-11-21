/** The routes of the app's area: projects: the board of a project and its list, the card by key, the page of a repository under its project, the tabs of a project, the project's settings (its tabs, and the script of a column in Columns), and the list of projects. Pages are fetched when their route opens. */
import { lazy } from "react";

const ProjectBoardRoute = lazy(() =>
  import("./ProjectBoardRoute").then((all) => ({ default: all.ProjectBoardRoute })),
);
const ProjectTabRoute = lazy(() =>
  import("../projects/ProjectTabRoute").then((all) => ({ default: all.ProjectTabRoute })),
);
const CardKeyPage = lazy(() =>
  import("../board/CardKeyPage").then((all) => ({ default: all.CardKeyPage })),
);
const ProjectPage = lazy(() =>
  import("../projects/ProjectPage").then((all) => ({ default: all.ProjectPage })),
);
const ProjectSettingsPage = lazy(() =>
  import("../projects/ProjectSettingsPage").then((all) => ({ default: all.ProjectSettingsPage })),
);
const AddRepositoryPage = lazy(() =>
  import("../projects/AddRepositoryPage").then((all) => ({ default: all.AddRepositoryPage })),
);
const ProjectsPage = lazy(() =>
  import("../projects/ProjectsPage").then((all) => ({ default: all.ProjectsPage })),
);

/** The routes of this area, as children of the shell's route. */
export const projectsRoutes = [
  { path: "projects", element: <ProjectsPage /> },
  { path: "projects/new", element: <AddRepositoryPage /> },
  { path: "projects/:projectId", element: <ProjectBoardRoute /> },
  { path: "projects/:projectId/workflows", element: <ProjectTabRoute tab="workflows" /> },
  { path: "projects/:projectId/cycles", element: <ProjectTabRoute tab="cycles" /> },
  { path: "projects/:projectId/milestones", element: <ProjectTabRoute tab="milestones" /> },
  { path: "projects/:projectId/intake", element: <ProjectTabRoute tab="intake" /> },
  { path: "projects/:projectId/list", element: <ProjectBoardRoute list /> },
  { path: "projects/:projectId/repositories/:repoId", element: <ProjectPage /> },
  { path: "projects/:projectId/settings/columns/:column", element: <ProjectSettingsPage /> },
  { path: "projects/:projectId/settings/:tab?", element: <ProjectSettingsPage /> },
  { path: "card/:key", element: <CardKeyPage /> },
];
