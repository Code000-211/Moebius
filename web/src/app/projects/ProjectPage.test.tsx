// @vitest-environment jsdom
/** `/projects/:projectId/repositories/:repoId`: the page of a repository the project holds, and the Notice for any other. */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ProjectPage } from "./ProjectPage";

// The panels read the repository's files and settings; only the page around them is under test.
vi.mock("./FilesPanel", () => ({ FilesPanel: () => null }));
vi.mock("./SettingsPanel", () => ({ SettingsPanel: () => null }));
vi.mock("./PluginSettings", () => ({ PluginSettings: () => null }));

const repository = (id: number, name: string) => ({
  id,
  name,
  path: `/srv/${name}`,
  forge: "none",
  default_branch: "main",
  home_project_id: 1,
  github: null,
  trusted: [],
});
const snapshot = {
  boards: [
    { repository: repository(11, "shop"), cards: [], waiting: {}, changes: [], round_limit: 3 },
    { repository: repository(12, "docs"), cards: [], waiting: {}, changes: [], round_limit: 3 },
  ],
  harness_changes: [],
  inbox: [],
};
const projects = [
  { id: 1, name: "Shop", repositories: [{ id: 11, name: "shop", home: true }], files: null },
  { id: 2, name: "Docs", repositories: [{ id: 12, name: "docs", home: true }], files: null },
];

beforeEach(() => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (path: string) => {
      if (path === "/v1/snapshot") return new Response(JSON.stringify(snapshot));
      if (path === "/v1/projects") return new Response(JSON.stringify(projects));
      return new Response("[]");
    }),
  );
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function open(address: string) {
  const router = createMemoryRouter(
    [{ path: "projects/:projectId/repositories/:repoId", element: <ProjectPage /> }],
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

describe("the page of a repository under its project", () => {
  it("opens a repository the project holds, with the board and settings of that project one link away", async () => {
    open("/projects/1/repositories/11");
    expect(await screen.findByRole("heading", { name: "shop" })).toBeTruthy();
    expect(screen.getByRole("link", { name: "Open board" }).getAttribute("href")).toBe(
      "/projects/1?repository=11",
    );
    expect(screen.getByRole("link", { name: "Project settings" }).getAttribute("href")).toBe(
      "/projects/1/settings",
    );
  });

  it("shows the Notice, and does not redirect, when the repository is not one of the project's", async () => {
    const router = open("/projects/1/repositories/12");
    const notice = await screen.findByRole("alert");
    expect(notice.textContent).toContain("This project has no repository with that address");
    expect(router.state.location.pathname).toBe("/projects/1/repositories/12");
    expect(router.state.historyAction).toBe("POP");
  });

  it("shows the Notice for a repository nobody has", async () => {
    const router = open("/projects/1/repositories/99");
    expect((await screen.findByRole("alert")).textContent).toContain("no repository");
    expect(router.state.location.pathname).toBe("/projects/1/repositories/99");
  });
});
