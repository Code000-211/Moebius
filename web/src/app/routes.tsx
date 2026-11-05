/**
 * The routes: every page of `pages.ts` under the shell, the board of a repository with the script editor of a column, the run a card links to, the Terminal and Ports pages, and the redirect from `/` to the
 * first page. The server answers `index.html` for these paths, so a page can be opened and reloaded by its address.
 *
 * Every page is fetched when its route opens rather than with the first byte of the app, which is what lets the shell
 * stay small: a person who opens the Inbox pays nothing for xterm, the diff viewer, or the editor of a column script.
 * The shell waits for a page with the loader of [`Shell`](./shell/Shell.tsx).
 */
import { lazy } from "react";
import { createBrowserRouter, Navigate } from "react-router";
import { App } from "./App";
import { DEFAULT_PAGE, PAGES } from "./pages";
import { harnessRoutes } from "./routesB";
import { NotFoundPage, PlaceholderPage } from "./shell/Page";

const AutomationsPage = lazy(() =>
  import("./automations/AutomationsPage").then((all) => ({ default: all.AutomationsPage })),
);
const BoardPage = lazy(() =>
  import("./board/BoardPage").then((all) => ({ default: all.BoardPage })),
);
const ScriptPage = lazy(() =>
  import("./board/ScriptPage").then((all) => ({ default: all.ScriptPage })),
);
const DecisionModelPage = lazy(() =>
  import("./decision/DecisionModelPage").then((all) => ({ default: all.DecisionModelPage })),
);
const ExperimentsPage = lazy(() =>
  import("./experiments/ExperimentsPage").then((all) => ({ default: all.ExperimentsPage })),
);
const FleetPage = lazy(() =>
  import("./fleet/FleetPage").then((all) => ({ default: all.FleetPage })),
);
const InboxPage = lazy(() =>
  import("./inbox/InboxPage").then((all) => ({ default: all.InboxPage })),
);
const PluginSettingsPage = lazy(() =>
  import("./plugins/PluginSettingsPage").then((all) => ({ default: all.PluginSettingsPage })),
);
const PortsPage = lazy(() =>
  import("./ports/PortsPage").then((all) => ({ default: all.PortsPage })),
);
const AddRepositoryPage = lazy(() =>
  import("./projects/AddRepositoryPage").then((all) => ({ default: all.AddRepositoryPage })),
);
const ProjectPage = lazy(() =>
  import("./projects/ProjectPage").then((all) => ({ default: all.ProjectPage })),
);
const ProjectsPage = lazy(() =>
  import("./projects/ProjectsPage").then((all) => ({ default: all.ProjectsPage })),
);
const RetroPage = lazy(() =>
  import("./retro/RetroPage").then((all) => ({ default: all.RetroPage })),
);
const RulesPage = lazy(() =>
  import("./rules/RulesPage").then((all) => ({ default: all.RulesPage })),
);
const RunPage = lazy(() => import("./runs/RunPage").then((all) => ({ default: all.RunPage })));
const SettingsPage = lazy(() =>
  import("./settings/SettingsPage").then((all) => ({ default: all.SettingsPage })),
);
const StartPage = lazy(() =>
  import("./start/StartPage").then((all) => ({ default: all.StartPage })),
);
const TerminalPage = lazy(() =>
  import("./terminal/TerminalPage").then((all) => ({ default: all.TerminalPage })),
);
const UsagePage = lazy(() =>
  import("./usage/UsagePage").then((all) => ({ default: all.UsagePage })),
);
const ChatPage = lazy(() => import("../chat/ChatPage").then((all) => ({ default: all.ChatPage })));

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
  "inbox",
  "fleet",
  "projects",
  "usage",
  "start",
  "settings",
  "automations",
  "rules",
  "retro",
  "experiments",
  "decision-model",
]);

export const routes = [
  {
    path: "/",
    element: <App />,
    children: [
      { index: true, element: <Navigate to={`/${DEFAULT_PAGE}`} replace /> },
      ...harnessRoutes,
      ...PAGES.filter((page) => !PAGES_BUILT.has(page.path)).map((page) => ({
        path: page.path,
        element: <PlaceholderPage page={page} />,
      })),
      { path: "inbox", element: <InboxPage /> },
      { path: "fleet", element: <FleetPage /> },
      { path: "projects", element: <ProjectsPage /> },
      { path: "projects/new", element: <AddRepositoryPage /> },
      { path: "projects/:id", element: <ProjectPage /> },
      { path: "usage", element: <UsagePage /> },
      { path: "start", element: <StartPage /> },
      { path: "settings", element: <SettingsPage /> },
      { path: "plugin-settings", element: <PluginSettingsPage /> },
      { path: "chat", element: <ChatPage /> },
      { path: "terminal", element: <TerminalPage /> },
      { path: "ports", element: <PortsPage /> },
      { path: "automations", element: <AutomationsPage /> },
      { path: "rules", element: <RulesPage /> },
      { path: "retro", element: <RetroPage /> },
      { path: "experiments", element: <ExperimentsPage /> },
      { path: "decision-model", element: <DecisionModelPage /> },
      { path: "chat/:id", element: <ChatPage /> },
      { path: "board/:id", element: <BoardPage /> },
      { path: "board/:id/scripts/:column", element: <ScriptPage /> },
      { path: "runs/:id", element: <RunPage /> },
      { path: "*", element: <NotFoundPage /> },
    ],
  },
];

export const router = () => createBrowserRouter(routes);
