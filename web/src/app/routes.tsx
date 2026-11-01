/**
 * The routes: every page of `pages.ts` under the shell, the board of a repository with the read-only script of a column and the run a card links to, and the redirect from `/` to the
 * first page. The server answers `index.html` for these paths, so a page can be opened and reloaded by its address.
 */
import { createBrowserRouter, Navigate } from "react-router";
import { ChatPage } from "../chat/ChatPage";
import { App } from "./App";
import { BoardPage } from "./board/BoardPage";
import { RunPage } from "./board/RunPage";
import { ScriptPage } from "./board/ScriptPage";
import { DEFAULT_PAGE, PAGES } from "./pages";
import { NotFoundPage, PlaceholderPage } from "./shell/Page";

export const routes = [
  {
    path: "/",
    element: <App />,
    children: [
      { index: true, element: <Navigate to={`/${DEFAULT_PAGE}`} replace /> },
      ...PAGES.filter((page) => page.path !== "chat").map((page) => ({
        path: page.path,
        element: <PlaceholderPage page={page} />,
      })),
      { path: "chat", element: <ChatPage /> },
      { path: "chat/:id", element: <ChatPage /> },
      { path: "board/:id", element: <BoardPage /> },
      { path: "board/:id/scripts/:column", element: <ScriptPage /> },
      { path: "runs/:id", element: <RunPage /> },
      { path: "*", element: <NotFoundPage /> },
    ],
  },
];

export const router = () => createBrowserRouter(routes);
