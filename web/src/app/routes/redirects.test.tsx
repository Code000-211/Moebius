// @vitest-environment jsdom
/** Every address of the legacy table leads where the issue says, with the query kept; the old repository page too. */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { createMemoryRouter, RouterProvider, useLocation } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Project } from "../../api/types/projects";
import { LegacyRedirect } from "./LegacyRedirect";
import { ProjectBoardRoute } from "./ProjectBoardRoute";
import { LEGACY_ROUTES } from "./redirects";

// The board itself is not under test: the route only has to decide that it is the board.
vi.mock("../board/BoardPage", () => ({
  BoardPage: ({ project, list }: { project: Project; list?: boolean }) => (
    <p data-testid="board">{`board of ${project.id}${list ? " as a list" : ""}`}</p>
  ),
}));

const project = (id: number, repositories: number[], extra: Partial<Project> = {}): Project =>
  ({
    id,
    name: `P${id}`,
    key: `P${id}`,
    created_at: id,
    repositories: repositories.map((r) => ({ id: r, name: `r${r}`, home: false })),
    members: [],
    open_cards: 0,
    files: null,
    kinds: [],
    intake: false,
    private: false,
    member: true,
    ...extra,
  }) as Project;

let projects: Project[];

beforeEach(() => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response(JSON.stringify(projects))),
  );
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function Where() {
  const location = useLocation();
  return <p data-testid="where">{location.pathname + location.search}</p>;
}

/** The legacy table and the project board route as `routes.tsx` mounts them, at `address`. */
async function open(address: string) {
  const router = createMemoryRouter(
    [
      ...LEGACY_ROUTES.map((route) => ({
        path: route.path,
        element: <LegacyRedirect route={route} />,
      })),
      { path: "projects/:projectId", element: <ProjectBoardRoute /> },
      { path: "projects/:projectId/list", element: <ProjectBoardRoute list /> },
      { path: "*", element: <Where /> },
    ],
    { initialEntries: [address] },
  );
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return router;
}

const at = (router: Awaited<ReturnType<typeof open>>) =>
  router.state.location.pathname + router.state.location.search;

describe("the legacy table", () => {
  it("sends /board/:repoId to the board of the repository's home project, keeping ?card=", async () => {
    projects = [project(2, [5, 6]), project(3, [7])];
    const router = await open("/board/6?card=11");
    expect(await screen.findByTestId("board")).toBeTruthy();
    expect(at(router)).toBe("/projects/2?repository=6&card=11");
    // Replaced, not pushed: Back does not return to the old address.
    expect(router.state.historyAction).toBe("REPLACE");
  });

  it("sends a repository in several projects to the oldest project that is not private", async () => {
    projects = [
      project(1, [9], { private: true, created_at: 1 }),
      project(3, [9], { created_at: 30 }),
      project(2, [9], { created_at: 20 }),
    ];
    const router = await open("/board/9");
    await screen.findByTestId("board");
    expect(at(router)).toBe("/projects/2?repository=9");
  });

  it("falls back to a private project only when every project that holds the repository is private", async () => {
    projects = [project(4, [9], { private: true, created_at: 5 })];
    const router = await open("/board/9");
    await screen.findByTestId("board");
    expect(at(router)).toBe("/projects/4?repository=9");
  });

  it("finds a repository held as a project's own files", async () => {
    projects = [project(6, [], { files: 12 })];
    const router = await open("/board/12?card=3");
    await screen.findByTestId("board");
    expect(at(router)).toBe("/projects/6?repository=12&card=3");
  });

  it("sends /board/:repoId/scripts/:column to the editor of the column in the home project", async () => {
    projects = [project(2, [5])];
    const router = await open("/board/5/scripts/In%20progress");
    expect((await screen.findByTestId("where")).textContent).toBe(
      "/projects/2/settings/columns/In%20progress?repository=5",
    );
    expect(at(router)).toBe("/projects/2/settings/columns/In%20progress?repository=5");
  });

  it("opens the script of the project the dock named when it holds the repository", async () => {
    projects = [project(2, [5]), project(3, [5])];
    await open("/board/5/scripts/Backlog?project=3");
    expect((await screen.findByTestId("where")).textContent).toBe(
      "/projects/3/settings/columns/Backlog?repository=5",
    );
  });

  it("shows the page for no address when no project holds the repository", async () => {
    projects = [project(2, [5])];
    const router = await open("/board/99?card=1");
    await screen.findByText("There is no such page");
    expect(at(router)).toBe("/board/99?card=1");
  });

  it("sends the old repository page /projects/:repoId to the repository page under its project", async () => {
    projects = [project(1, [11, 12]), project(2, [13])];
    const router = await open("/projects/13");
    expect((await screen.findByTestId("where")).textContent).toBe("/projects/2/repositories/13");
    expect(at(router)).toBe("/projects/2/repositories/13");
  });

  it("opens the board when the id is a project, even if a repository has the same number", async () => {
    projects = [project(1, [2]), project(2, [3])];
    const router = await open("/projects/2");
    expect((await screen.findByTestId("board")).textContent).toBe("board of 2");
    expect(at(router)).toBe("/projects/2");
  });

  it("opens the list of a project at /projects/:projectId/list", async () => {
    projects = [project(2, [3])];
    await open("/projects/2/list");
    expect((await screen.findByTestId("board")).textContent).toBe("board of 2 as a list");
  });

  it("says there is no such project for an id that is neither a project nor a repository", async () => {
    projects = [project(2, [3])];
    const router = await open("/projects/77");
    await screen.findByText("No such project");
    expect(at(router)).toBe("/projects/77");
  });
});
