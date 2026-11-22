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

/** Opens `address` and waits for the redirect to land on the page of no particular route, which shows its address. */
async function landed(address: string) {
  const router = await open(address);
  await screen.findByTestId("where");
  return at(router);
}

const at = (router: Awaited<ReturnType<typeof open>>) =>
  router.state.location.pathname + router.state.location.search;

describe("the legacy table", () => {
  it("sends /more, a list that is gone, to the Inbox", async () => {
    projects = [];
    const router = await open("/more");
    await screen.findByTestId("where");
    expect(at(router)).toBe("/inbox");
    expect(router.state.historyAction).toBe("REPLACE");
  });

  it("sends /runners and /ports, which are tabs of Fleet now, to them", async () => {
    projects = [];
    const router = await open("/runners");
    await screen.findByTestId("where");
    expect(at(router)).toBe("/fleet/runners");
    expect(router.state.historyAction).toBe("REPLACE");
    cleanup();
    expect(await landed("/ports?x=1")).toBe("/fleet/ports?x=1");
  });

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

  it.each([
    ["workflow", "columns"],
    ["kinds", ""],
    ["plugins", "repositories"],
    ["import", ""],
  ])("sends the old settings tab %s of a project to its new place", async (old, tab) => {
    projects = [project(2, [5])];
    const router = await open(`/projects/2/settings/${old}?x=1`);
    await screen.findByTestId("where");
    expect(at(router)).toBe(`/projects/2/settings${tab === "" ? "" : `/${tab}`}?x=1`);
    expect(router.state.historyAction).toBe("REPLACE");
  });

  it("shows the page for no address for an old settings tab of a project that is not there", async () => {
    projects = [project(2, [5])];
    await open("/projects/9/settings/workflow");
    await screen.findByText("There is no such page");
  });

  describe("the old Cycles, Milestones and Intake addresses", () => {
    beforeEach(() => window.localStorage.clear());

    it("lead to the tab of the project named in the query, keeping the rest of it", async () => {
      projects = [project(1, [5]), project(2, [6])];
      const router = await open("/milestones?project=2&milestone=4");
      await screen.findByTestId("where");
      expect(at(router)).toBe("/projects/2/milestones?milestone=4");
      expect(router.state.historyAction).toBe("REPLACE");
    });

    it("lead to the page of the cycle that was open", async () => {
      projects = [project(1, [5]), project(2, [6])];
      expect(await landed("/cycles?project=2&cycle=9")).toBe("/projects/2/cycles/9");
      cleanup();
      expect(await landed("/cycles/9?project=1")).toBe("/projects/1/cycles/9");
    });

    it("lead to the only project, or to the one opened last when there are several", async () => {
      projects = [project(3, [5])];
      expect(await landed("/intake")).toBe("/projects/3/intake");
      cleanup();
      projects = [project(3, [5]), project(4, [6])];
      window.localStorage.setItem("moebius.lastProject", "4");
      expect(await landed("/cycles")).toBe("/projects/4/cycles");
    });

    it("lead to the list of projects when no project can be told", async () => {
      projects = [project(3, [5]), project(4, [6])];
      expect(await landed("/milestones")).toBe("/projects");
      cleanup();
      window.localStorage.setItem("moebius.lastProject", "99");
      expect(await landed("/intake")).toBe("/projects");
    });

    describe("the Issues list", () => {
      it("leads to the view All issues with its filters and sort kept", async () => {
        projects = [project(1, [5])];
        expect(await landed("/issues")).toBe("/views/all-issues");
        cleanup();
        expect(
          await landed("/issues?project=2&kind=Bug&field.Severity=high&sort=asc%3AEstimate"),
        ).toBe("/views/all-issues?project=2&kind=Bug&field.Severity=high&sort=asc%3AEstimate");
        cleanup();
        expect(await landed("/issues?card=4")).toBe("/views/all-issues?card=4");
      });
    });
  });

  it("sends the Skills, MCP servers and Settings addresses of Build to their tabs, keeping the query", async () => {
    projects = [];
    for (const [old, now] of [
      ["/skills", "/agents/skills"],
      ["/skills?skill=house-style", "/agents/skills?skill=house-style"],
      ["/mcp-servers?server=docs", "/agents/mcp-servers?server=docs"],
      ["/settings/agents?agent=Coder", "/agents?agent=Coder"],
      ["/settings/skills", "/agents/skills"],
      ["/settings/mcp-servers", "/agents/mcp-servers"],
      ["/settings/rules", "/rules"],
    ] as const) {
      expect(await landed(old), old).toBe(now);
      cleanup();
    }
  });

  describe("the old Workflows and Automations addresses", () => {
    beforeEach(() => window.localStorage.clear());

    it("lead to the Workflows tab of the project named, opened last, or the only one", async () => {
      projects = [project(1, [5]), project(2, [6])];
      expect(await landed("/workflows?project=2")).toBe("/projects/2/workflows");
      cleanup();
      window.localStorage.setItem("moebius.lastProject", "1");
      expect(await landed("/workflows")).toBe("/projects/1/workflows");
      cleanup();
      window.localStorage.clear();
      projects = [project(7, [5])];
      expect(await landed("/workflows")).toBe("/projects/7/workflows");
    });

    it("lead to the definition and the canvas under the project, escaping the name", async () => {
      projects = [project(3, [5])];
      expect(await landed("/workflows/night%20build")).toBe("/projects/3/workflows/night%20build");
      cleanup();
      expect(await landed("/workflows/release/canvas")).toBe(
        "/projects/3/workflows/release/canvas",
      );
    });

    it("lead the Automations page to the Scheduled runs, keeping the automation that was open", async () => {
      projects = [project(3, [5])];
      expect(await landed("/automations?automation=4")).toBe(
        "/projects/3/workflows?runs=scheduled&automation=4",
      );
    });

    it("lead to the list of projects when no project can be told", async () => {
      projects = [project(3, [5]), project(4, [6])];
      expect(await landed("/workflows")).toBe("/projects");
      cleanup();
      expect(await landed("/automations")).toBe("/projects");
      cleanup();
      expect(await landed("/workflows/release")).toBe("/projects");
    });
  });
});
