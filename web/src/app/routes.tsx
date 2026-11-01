/**
 * The routes: every page of `pages.ts` under the shell, the board of a repository with the read-only script of a column, the run a card links to, the Terminal and Ports pages, and the redirect from `/` to the
 * first page. The server answers `index.html` for these paths, so a page can be opened and reloaded by its address.
 */
import { createBrowserRouter, Navigate } from "react-router";
import { ChatPage } from "../chat/ChatPage";
import { App } from "./App";
import { BoardPage } from "./board/BoardPage";
import { ScriptPage } from "./board/ScriptPage";
import { DEFAULT_PAGE, PAGES } from "./pages";
import { PortsPage } from "./ports/PortsPage";
import { RunPage } from "./runs/RunPage";
import { NotFoundPage, PlaceholderPage } from "./shell/Page";
import { TerminalPage } from "./terminal/TerminalPage";

/** The pages that have a route of their own below; the others show a placeholder. */
const BUILT = new Set(["chat", "terminal", "ports"]);

export const routes = [
  {
    path: "/",
    element: <App />,
    children: [
      { index: true, element: <Navigate to={`/${DEFAULT_PAGE}`} replace /> },
      ...PAGES.filter((page) => !BUILT.has(page.path)).map((page) => ({
        path: page.path,
        element: <PlaceholderPage page={page} />,
      })),
      { path: "chat", element: <ChatPage /> },
      { path: "terminal", element: <TerminalPage /> },
      { path: "ports", element: <PortsPage /> },
      { path: "chat/:id", element: <ChatPage /> },
      { path: "board/:id", element: <BoardPage /> },
      { path: "board/:id/scripts/:column", element: <ScriptPage /> },
      { path: "runs/:id", element: <RunPage /> },
      { path: "*", element: <NotFoundPage /> },
    ],
  },
];

export const router = () => createBrowserRouter(routes);
