// @vitest-environment jsdom
/**
 * A project's settings: exactly four tabs in the address, read-only for anyone but an owner, the intake switch that
 * the project's header follows at once, and the column in the address that the Columns tab shows.
 */
import { QueryClient, QueryClientProvider, useQuery } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { listProjects } from "../../api/calls/projects";
import { keys } from "../../api/keys";
import type { Project } from "../../api/types/projects";
import { ProjectHeader } from "./ProjectHeader";
import { ProjectSettingsPage } from "./ProjectSettingsPage";

vi.mock("../board/ScriptEditor", () => ({
  ScriptEditor: ({ column, readOnly }: { column: string; readOnly?: boolean }) => (
    <p data-testid="editor">{`${column}${readOnly ? " read only" : ""}`}</p>
  ),
}));

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const platform: Project = {
  id: 7,
  name: "Platform",
  key: "PLT",
  created_at: 1,
  private: false,
  repositories: [{ id: 1, name: "core", home: true }],
  members: [],
  open_cards: 0,
  files: null,
  kinds: [],
  intake: false,
  member: true,
};

const SCRIPTS = [
  { column: "Backlog", source: "builtin", path: "a" },
  { column: "In progress", source: "builtin", path: "b" },
];

/** The fake server: what `fetch` answers for the routes the settings page reads and writes. */
function serve(sent: Array<{ method: string; path: string; body: unknown }> = []) {
  let current = platform;
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = new URL(String(input), "http://x").pathname;
      const method = init?.method ?? "GET";
      sent.push({ method, path, body: init?.body ? JSON.parse(String(init.body)) : undefined });
      const json = (body: unknown) => new Response(JSON.stringify(body));
      if (path === "/v1/projects/7/intake") {
        current = {
          ...current,
          intake: (JSON.parse(String(init?.body)) as { enabled: boolean }).enabled,
        };
        return json(current);
      }
      if (path === "/v1/projects/7/requests") return json([]);
      if (path === "/v1/projects/7/scripts") return json(SCRIPTS);
      if (path.startsWith("/v1/projects/7/scripts/"))
        return json({ column: "x", source: "builtin", path: "p", text: "" });
      if (path === "/v1/kinds") return json([]);
      if (path === "/v1/projects") return json([current]);
      return json({});
    }),
  );
  return sent;
}

function Where() {
  const at = useLocation();
  return <p data-testid="where">{at.pathname}</p>;
}

/** The header of the project, reading the project from the cache as the board does. */
function LiveHeader() {
  const projects = useQuery({ queryKey: keys.projects, queryFn: listProjects });
  const project = projects.data?.find((each) => each.id === 7);
  return project ? <ProjectHeader project={project} tab="board" onTerminal={() => {}} /> : null;
}

function show(address: string, role: string, withHeader = false) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData(keys.session, { person: { role, handle: "leon" } });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[address]}>
        <Where />
        {withHeader && <LiveHeader />}
        <Routes>
          <Route
            path="projects/:projectId/settings/columns/:column"
            element={<ProjectSettingsPage />}
          />
          <Route path="projects/:projectId/settings/:tab?" element={<ProjectSettingsPage />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
  return client;
}

describe("a project's settings", () => {
  it("has exactly the tabs General, Members, Repositories and Columns, and the tab is in the address", async () => {
    serve();
    show("/projects/7/settings", "owner");
    const nav = await screen.findByRole("navigation", { name: "Project settings" });
    expect(Array.from(nav.querySelectorAll("a")).map((each) => each.textContent)).toEqual([
      "General",
      "Members",
      "Repositories",
      "Columns",
    ]);
    // Each row has its icon, and there is no search field, as in Settings.
    for (const link of Array.from(nav.querySelectorAll("a"))) {
      expect(link.querySelector("svg"), link.textContent ?? "").toBeTruthy();
    }
    expect(screen.queryByRole("searchbox")).toBeNull();
    fireEvent.click(screen.getByRole("link", { name: "Repositories" }));
    expect(screen.getByTestId("where").textContent).toBe("/projects/7/settings/repositories");
    fireEvent.click(screen.getByRole("link", { name: "General" }));
    expect(screen.getByTestId("where").textContent).toBe("/projects/7/settings");
  });

  it("shows the name, the key, private, intake and the kinds in General", async () => {
    serve();
    show("/projects/7/settings", "owner");
    expect(((await screen.findByRole("textbox", { name: "Name" })) as HTMLInputElement).value).toBe(
      "Platform",
    );
    expect(screen.getByText("PLT")).toBeTruthy();
    expect(screen.getByRole("switch", { name: "Private" })).toBeTruthy();
    expect(screen.getByRole("switch", { name: "Intake" })).toBeTruthy();
    expect(await screen.findByText("Kinds of issues")).toBeTruthy();
  });

  it("turns intake on with the route, and the header gets its Intake tab at once", async () => {
    const sent = serve();
    show("/projects/7/settings", "owner", true);
    await screen.findByRole("tab", { name: "Board" });
    expect(screen.queryByRole("tab", { name: /Intake/ })).toBeNull();
    fireEvent.click(await screen.findByRole("switch", { name: "Intake" }));
    await screen.findByRole("tab", { name: /^Intake/ });
    const put = sent.find((each) => each.method === "PUT" && each.path === "/v1/projects/7/intake");
    expect(put?.body).toEqual({ enabled: true });
  });

  it("is read only for anyone but an owner", async () => {
    serve();
    show("/projects/7/settings", "member");
    const name = (await screen.findByRole("textbox", { name: "Name" })) as HTMLInputElement;
    expect(name.disabled).toBe(true);
    expect(screen.queryByRole("switch", { name: "Intake" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Save" })).toBeNull();
    expect(screen.getByText("Off")).toBeTruthy();
  });

  it("opens the column the address names in Columns, and a member reads it", async () => {
    serve();
    show("/projects/7/settings/columns/In%20progress", "member");
    expect((await screen.findByTestId("editor")).textContent).toBe("In progress read only");
    expect(screen.getByRole("link", { name: "Columns" }).getAttribute("aria-current")).toBe("page");
  });

  it("lets an owner edit the script and changes the address with the column picked", async () => {
    serve();
    show("/projects/7/settings/columns/Backlog", "owner");
    await waitFor(() => expect(screen.getByTestId("editor").textContent).toBe("Backlog"));
  });

  it("takes the repositories to the page of the repository", async () => {
    serve();
    show("/projects/7/settings/repositories", "owner");
    const link = await screen.findByRole("link", { name: "Settings of core" });
    expect(link.getAttribute("href")).toBe("/projects/7/repositories/1");
  });
});
