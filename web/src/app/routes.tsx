/**
 * The routes: every page of `pages.ts` under the shell, the board of a repository with the script editor of a column, the run a card links to, the Terminal and Ports pages, and the redirect from `/` to the
 * first page. The server answers `index.html` for these paths, so a page can be opened and reloaded by its address.
 */
import { createBrowserRouter, Navigate } from "react-router";
import { ChatPage } from "../chat/ChatPage";
import { App } from "./App";
import { AutomationsPage } from "./automations/AutomationsPage";
import { BoardPage } from "./board/BoardPage";
import { ScriptPage } from "./board/ScriptPage";
import { DecisionModelPage } from "./decision/DecisionModelPage";
import { ExperimentsPage } from "./experiments/ExperimentsPage";
import { FleetPage } from "./fleet/FleetPage";
import { InboxPage } from "./inbox/InboxPage";
import { DEFAULT_PAGE, PAGES } from "./pages";
import { PortsPage } from "./ports/PortsPage";
import { AddRepositoryPage } from "./projects/AddRepositoryPage";
import { ProjectPage } from "./projects/ProjectPage";
import { ProjectsPage } from "./projects/ProjectsPage";
import { RetroPage } from "./retro/RetroPage";
import { harnessRoutes } from "./routesB";
import { RulesPage } from "./rules/RulesPage";
import { RunPage } from "./runs/RunPage";
import { SettingsPage } from "./settings/SettingsPage";
import { NotFoundPage, PlaceholderPage } from "./shell/Page";
import { StartPage } from "./start/StartPage";
import { TerminalPage } from "./terminal/TerminalPage";
import { UsagePage } from "./usage/UsagePage";

/** The pages that have their own route below; the others show a placeholder. */
const PAGES_BUILT = new Set([
  "chat",
  "agents",
  "skills",
  "mcp-servers",
  "models",
  "plugins",
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
