/**
 * The routes: every place of the table in `nav.ts` under the shell, the redirect from `/` to the first page, and the small window
 * of the desktop app. The pages are in one file per area under `routes/` (`projects`, `fleet`, `workflows`,
 * `workspace`, `build`, `improve`, `settings`), each exporting its route objects; `routes/redirects.ts` is the one table of the
 * addresses that moved. The server answers `index.html` for these paths, so a page can be opened and reloaded by its
 * address.
 *
 * Every page is fetched when its route opens rather than with the first byte of the app, which is what lets the shell
 * stay small: a person who opens the Inbox pays nothing for xterm, the diff viewer, or the editor of a column script.
 * The shell waits for a page with the loader of [`Shell`](./shell/Shell.tsx).
 */
import { lazy, Suspense } from "react";
import { createBrowserRouter, Navigate } from "react-router";
import { App } from "./App";
import { DEFAULT_PAGE } from "./nav";
import { buildRoutes } from "./routes/build";
import { fleetRoutes } from "./routes/fleet";
import { improveRoutes } from "./routes/improve";
import { LegacyRedirect } from "./routes/LegacyRedirect";
import { projectsRoutes } from "./routes/projects";
import { LEGACY_ROUTES } from "./routes/redirects";
import { settingsRoutes } from "./routes/settings";
import { workflowsRoutes } from "./routes/workflows";
import { workspaceRoutes } from "./routes/workspace";
import { NotFoundPage } from "./shell/Page";

const QuickWindow = lazy(() =>
  import("../desktop/QuickWindow").then((all) => ({ default: all.QuickWindow })),
);

export const routes = [
  // The small window of the desktop app's global shortcut: outside the shell, it is a window of its own.
  {
    path: "/quick",
    element: (
      <Suspense fallback={null}>
        <QuickWindow />
      </Suspense>
    ),
  },
  {
    path: "/",
    element: <App />,
    children: [
      { index: true, element: <Navigate to={`/${DEFAULT_PAGE}`} replace /> },
      ...settingsRoutes,
      ...fleetRoutes,
      ...workspaceRoutes,
      ...projectsRoutes,
      ...workflowsRoutes,
      ...buildRoutes,
      ...improveRoutes,
      ...LEGACY_ROUTES.map((route) => ({
        path: route.path,
        element: <LegacyRedirect route={route} />,
      })),
      { path: "*", element: <NotFoundPage /> },
    ],
  },
];

export const router = () => createBrowserRouter(routes);
