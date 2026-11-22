/**
 * The Agents page as a list of 36px rows, each a link to the agent's own page; "New agent" asks blank or from an agent
 * that exists and goes to the page of a new agent, and the old addresses (`?agent=`, `?new=1&blank=1`) lead to the pages
 * that replaced them.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, useLocation } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { AgentsPage } from "./AgentsPage";

const ROUTES: Record<string, unknown> = {
  "/v1/skills": [{ name: "moebius", description: "Operate Moebius.", built_in: true }],
  "/v1/mcp-servers": [],
  "/v1/runners": [],
  "/v1/models": { upstreams: [], agents: [], teacher: null, decision_model: null },
  "/v1/models/available": { models: [], errors: [], classes: [], decision_models: [] },
  "/v1/agents": [
    {
      name: "Moebius",
      model: "fake-small",
      protocol: "openai",
      harness: "pi-bolt",
      built_in: true,
    },
    { name: "Coder", model: "fake-small", protocol: "openai", harness: "pi-bolt", built_in: false },
    {
      name: "Reviewer",
      model: "fake-small",
      protocol: "openai",
      harness: "pi-bolt",
      built_in: false,
    },
  ],
  "/v1/agents/Coder": {
    name: "Coder",
    settings: { model: "fake-small", protocol: "openai", skills: [], extensions: [], mcp: [] },
    instructions: "Keep changes tiny.",
  },
  "/v1/agents/Reviewer": {
    name: "Reviewer",
    settings: { model: "fake-small", protocol: "openai", skills: [], extensions: [], mcp: [] },
    instructions: "Say what is wrong.",
  },
};

/** The requests that were not reads, as `[method, path, body]`. */
let writes: Array<[string, string, unknown]> = [];

function Where() {
  const location = useLocation();
  return <p data-testid="where">{location.pathname + location.search}</p>;
}

function show(start = "/agents", routes: Record<string, unknown> = ROUTES) {
  writes = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (path: string, init?: { method?: string; body?: string }) => {
      const route = path.split("?")[0] ?? path;
      if (init?.method && init.method !== "GET") {
        const body = init.body ? JSON.parse(init.body) : undefined;
        writes.push([init.method, route, body]);
        return { ok: true, status: 200, text: async () => JSON.stringify({ name: "x", ...body }) };
      }
      return { ok: true, status: 200, text: async () => JSON.stringify(routes[route] ?? []) };
    }),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[start]}>
        <AgentsPage />
        <Where />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

afterEach(() => vi.unstubAllGlobals());

describe("the agents as a list of rows", () => {
  it("lists every agent as one row that is a link to its page", async () => {
    show("/agents");
    const rows = await screen.findAllByTestId("agent-row");
    expect(rows.map((row) => row.textContent)).toEqual([
      expect.stringContaining("Moebius"),
      expect.stringContaining("Coder"),
      expect.stringContaining("Reviewer"),
    ]);
    expect(rows[0]?.textContent).toContain("Built in");
    expect(rows[1]?.textContent).toContain("fake-small · openai");
    expect(screen.queryByTestId("editor-window")).toBeNull();
    expect(
      (await screen.findByRole("link", { name: "Open agent Reviewer" })).getAttribute("href"),
    ).toBe("/agents/Reviewer");
  });

  it("opens the page of the agent a row names", async () => {
    show("/agents");
    fireEvent.click(await screen.findByRole("link", { name: "Open agent Reviewer" }));
    expect(screen.getByTestId("where").textContent).toBe("/agents/Reviewer");
  });

  it("leads the old address ?agent= to the page of that agent, and ?new=1&blank=1 to the page of a new one", async () => {
    show("/agents?agent=Coder");
    await waitFor(() => expect(screen.getByTestId("where").textContent).toBe("/agents/Coder"));
    cleanup();
    show("/agents?new=1&blank=1");
    await waitFor(() =>
      expect(screen.getByTestId("where").textContent).toBe("/agents/new?blank=1"),
    );
  });
});

describe("what a page that cannot be read says", () => {
  it("names the failure beside the buttons and offers to read again, instead of saying there are no agents", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (path: string) =>
        (path.split("?")[0] ?? path) === "/v1/skills"
          ? { ok: false, status: 503, text: async () => "the harness repository is on no remote" }
          : { ok: false, status: 503, text: async () => "the agents are not readable yet" },
      ),
    );
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      <QueryClientProvider client={client}>
        <MemoryRouter initialEntries={["/agents"]}>
          <AgentsPage />
        </MemoryRouter>
      </QueryClientProvider>,
    );
    const told = await screen.findAllByRole("alert");
    const said = told.map((each) => each.textContent ?? "").join(" ");
    expect(said).toContain("the harness repository is on no remote");
    expect(said).toContain("the agents are not readable yet");
    expect(screen.getAllByRole("button", { name: "Try again" })).toHaveLength(2);
    expect(
      screen.getByText(
        "The harness configuration could not be read, so no agent can be added yet.",
      ),
    ).toBeTruthy();
    expect(screen.getByRole("button", { name: "New agent" }).hasAttribute("disabled")).toBe(true);
    // An empty list would be the opposite of what happened.
    expect(screen.queryByText("No agents yet")).toBeNull();
  });
});

describe("a new agent", () => {
  it("first asks for blank or an existing agent, and goes to the page of a blank one", async () => {
    show("/agents");
    const button = (await screen.findByRole("button", { name: "New agent" })) as HTMLButtonElement;
    await vi.waitFor(() => expect(button.disabled).toBe(false));
    fireEvent.click(button);
    expect(await screen.findByRole("dialog", { name: "New agent" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "From an existing agent" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await waitFor(() =>
      expect(screen.getByTestId("where").textContent).toBe("/agents/new?blank=1"),
    );
  });

  it("goes to the page of a copy of the agent chosen", async () => {
    show("/agents?new=1");
    fireEvent.click(await screen.findByRole("button", { name: "From an existing agent" }));
    // The agents it can start from are on their way.
    await waitFor(() =>
      expect((screen.getByRole("button", { name: "Continue" }) as HTMLButtonElement).disabled).toBe(
        false,
      ),
    );
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await waitFor(() =>
      expect(screen.getByTestId("where").textContent).toBe("/agents/new?from=Coder"),
    );
  });
});
