/**
 * The routes: every page of `pages.ts` under the shell, the redirect from `/` to the first page, and the small window
 * of the desktop app. The pages are in one file per area under `routes/` (`projects`, `fleet`, `workflows`,
 * `workspace`, `improve`, `settings`), each exporting its route objects; `routes/redirects.ts` is the one table of the
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
import { DEFAULT_PAGE, PAGES } from "./pages";
import { fleetRoutes } from "./routes/fleet";
import { improveRoutes } from "./routes/improve";
import { LegacyRedirect } from "./routes/LegacyRedirect";
import { projectsRoutes } from "./routes/projects";
import { LEGACY_ROUTES } from "./routes/redirects";
import { settingsRoutes } from "./routes/settings";
import { workflowsRoutes } from "./routes/workflows";
import { workspaceRoutes } from "./routes/workspace";
import { NotFoundPage, PlaceholderPage } from "./shell/Page";

const QuickWindow = lazy(() =>
  import("../desktop/QuickWindow").then((all) => ({ default: all.QuickWindow })),
);

/** The pages that have their own route below; the others show a placeholder. */
const PAGES_BUILT = new Set([
  "chat",
  "agents",
  "skills",
  "mcp-servers",
  "models",
  "plugins",
  "plugin-settings",
  "terminal",
  "ports",
  "shortcuts",
  "inbox",
  "fleet",
  "projects",
  "milestones",
  "views",
  "cycles",
  "analytics",
  "intake",
  "usage",
  "start",
  "settings",
  "automations",
  "rules",
  "retro",
  "experiments",
  "decision-model",
  "workflows",
  "people",
  "issues",
  "my-issues",
  "kinds",
  "tokens",
  "wiki",
  "more",
]);

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
      ...PAGES.filter((page) => !PAGES_BUILT.has(page.path)).map((page) => ({
        path: page.path,
        element: <PlaceholderPage page={page} />,
      })),
      ...settingsRoutes,
      ...fleetRoutes,
      ...workspaceRoutes,
      ...projectsRoutes,
      ...workflowsRoutes,
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
