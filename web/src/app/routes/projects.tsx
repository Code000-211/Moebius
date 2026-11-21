/** The routes of the app's area: projects: the board of a project and its list, the card by key, the page of a repository under its project, the script editor of a column, the project's settings, and the list of projects. Pages are fetched when their route opens. */
import { lazy } from "react";

const ProjectBoardRoute = lazy(() =>
  import("./ProjectBoardRoute").then((all) => ({ default: all.ProjectBoardRoute })),
);
const CardKeyPage = lazy(() =>
  import("../board/CardKeyPage").then((all) => ({ default: all.CardKeyPage })),
);
const ScriptPage = lazy(() =>
  import("../board/ScriptPage").then((all) => ({ default: all.ScriptPage })),
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
  { path: "projects/:projectId/list", element: <ProjectBoardRoute list /> },
  { path: "projects/:projectId/repositories/:repoId", element: <ProjectPage /> },
  { path: "projects/:projectId/settings/columns/:column", element: <ScriptPage /> },
  { path: "projects/:projectId/settings/:tab?", element: <ProjectSettingsPage /> },
  { path: "card/:key", element: <CardKeyPage /> },
];
