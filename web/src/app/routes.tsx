/**
 * The routes: every page of `pages.ts` under the shell, the board of a repository, and the redirect from `/` to the
 * first page. The server answers `index.html` for these paths, so a page can be opened and reloaded by its address.
 */
import { createBrowserRouter, Navigate } from "react-router";
import { App } from "./App";
import { DEFAULT_PAGE, PAGES } from "./pages";
import { BoardPage, NotFoundPage, PlaceholderPage } from "./shell/Page";

export const routes = [
  {
    path: "/",
    element: <App />,
    children: [
      { index: true, element: <Navigate to={`/${DEFAULT_PAGE}`} replace /> },
      ...PAGES.map((page) => ({ path: page.path, element: <PlaceholderPage page={page} /> })),
      { path: "board/:id", element: <BoardPage /> },
      { path: "*", element: <NotFoundPage /> },
    ],
  },
];

export const router = () => createBrowserRouter(routes);
