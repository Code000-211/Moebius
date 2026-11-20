/**
 * The Agents page as a list of 36px rows, each opening its agent in the editor window: `?agent=` and `?new=1` open it,
 * a change is guarded when the window closes, a save writes the agent and closes the window, and a new agent starts
 * with its choice.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
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

function show(start = "/agents?agent=Coder", routes: Record<string, unknown> = ROUTES) {
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
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

afterEach(() => vi.unstubAllGlobals());

/** Save, the one of the editor open now; being disabled is how it says there is nothing to write. */
const saveButton = () => screen.getByRole("button", { name: "Save" }) as HTMLButtonElement;

describe("the agents as a list of rows", () => {
  it("lists every agent as one row and opens no window by itself", async () => {
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
  });

  it("opens the window of the agent a row names, and puts it in the address", async () => {
    show("/agents");
    fireEvent.click(await screen.findByRole("button", { name: "Edit agent Reviewer" }));
    const window = await screen.findByTestId("editor-window");
    expect(window).toBeTruthy();
    expect(screen.getByRole("dialog", { name: "Edit agent Reviewer" })).toBeTruthy();
    expect(await screen.findByDisplayValue("Say what is wrong.")).toBeTruthy();
  });

  it("opens the window of the agent ?agent= names", async () => {
    show();
    expect(await screen.findByDisplayValue("Keep changes tiny.")).toBeTruthy();
    expect(screen.getByRole("dialog", { name: "Edit agent Coder" })).toBeTruthy();
  });
});

describe("the draft of an agent", () => {
  it("is guarded when the window closes, and Keep editing leaves it as typed", async () => {
    show();
    const model = await screen.findByLabelText("Model");
    fireEvent.change(model, { target: { value: "qwen-local" } });
    expect(saveButton().disabled).toBe(false);
    fireEvent.keyDown(screen.getByTestId("editor-window"), { key: "Escape" });
    expect(screen.getByRole("alertdialog", { name: "Discard changes?" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Keep editing" }));
    expect((screen.getByLabelText("Model") as HTMLInputElement).value).toBe("qwen-local");
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    fireEvent.click(screen.getByRole("button", { name: "Discard" }));
    await waitFor(() => expect(screen.queryByTestId("editor-window")).toBeNull());
    // Dropped for good: the agent opens again as the file has it.
    fireEvent.click(screen.getByRole("button", { name: "Edit agent Coder" }));
    expect(((await screen.findByLabelText("Model")) as HTMLInputElement).value).toBe("fake-small");
  });

  it("closes a clean window at once, and Save rests until there is a change", async () => {
    show();
    await screen.findByLabelText("Model");
    expect(saveButton().disabled).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByTestId("editor-window")).toBeNull());
  });

  it("saves with the usual PUT and closes the window", async () => {
    show();
    fireEvent.change(await screen.findByLabelText("Model"), { target: { value: "qwen-local" } });
    fireEvent.click(saveButton());
    await vi.waitFor(() => expect(writes).toHaveLength(1));
    expect(writes[0]?.[0]).toBe("PUT");
    expect(writes[0]?.[1]).toBe("/v1/agents/Coder");
    expect(writes[0]?.[2]).toMatchObject({ settings: { model: "qwen-local" } });
    await waitFor(() => expect(screen.queryByTestId("editor-window")).toBeNull());
  });

  it("steps aside for the package sheet and comes back with what was typed", async () => {
    show();
    fireEvent.change(await screen.findByLabelText("Model"), { target: { value: "qwen-local" } });
    fireEvent.click(screen.getByRole("button", { name: "Browse Pi packages" }));
    await waitFor(() => expect(screen.queryByTestId("editor-window")).toBeNull());
    fireEvent.click(screen.getByRole("button", { name: "Edit agent Coder" }));
    expect(((await screen.findByLabelText("Model")) as HTMLInputElement).value).toBe("qwen-local");
    expect(saveButton().disabled).toBe(false);
  });

  it("deletes after asking", async () => {
    show();
    await screen.findByLabelText("Model");
    fireEvent.click(screen.getByRole("button", { name: "Delete agent" }));
    expect(await screen.findByText("Delete this agent?")).toBeTruthy();
    expect(writes).toEqual([]);
    fireEvent.click(screen.getAllByRole("button", { name: "Delete agent" }).at(-1) as HTMLElement);
    await vi.waitFor(() => expect(writes).toHaveLength(1));
    expect(writes[0]?.[0]).toBe("DELETE");
    expect(writes[0]?.[1]).toBe("/v1/agents/Coder");
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
  const routes: Record<string, unknown> = {
    ...ROUTES,
    "/v1/agents": [
      { name: "Moebius", model: "m", protocol: "openai", harness: "pi-bolt", built_in: true },
      { name: "Coder", model: "", protocol: "openai", harness: "pi-bolt", built_in: true },
      { name: "Reviewer", model: "fast", protocol: "openai", harness: "pi-bolt", built_in: true },
      { name: "Fast", model: "small", protocol: "openai", harness: "pi-bolt", built_in: false },
    ],
    "/v1/agents/Coder": {
      name: "Coder",
      built_in: {
        instructions: "You are the Coder.",
        skills: [],
        engineering_skills: ["tdd", "diagnosing-bugs", "moebius-autonomous"].map((name) => ({
          name,
          description: "",
        })),
      },
    },
    "/v1/agents/Fast": {
      name: "Fast",
      settings: { model: "small", protocol: "openai", skills: [], extensions: [], mcp: ["docs"] },
      instructions: "Be brief.",
    },
  };

  /** The New agent button, once the page has read the harness configuration and it is on. */
  async function newAgent() {
    const button = (await screen.findByRole("button", { name: "New agent" })) as HTMLButtonElement;
    await vi.waitFor(() => expect(button.disabled).toBe(false));
    fireEvent.click(button);
  }

  it("first asks for blank or an existing agent, and starts blank", async () => {
    show("/agents", routes);
    await newAgent();
    expect(await screen.findByRole("dialog", { name: "New agent" })).toBeTruthy();
    expect(await screen.findByRole("button", { name: "Blank" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "From an existing agent" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    const name = await screen.findByLabelText("Name of the new agent");
    expect((name as HTMLInputElement).value).toBe("");
    expect((screen.getByLabelText("Model") as HTMLInputElement).value).toBe("");
  });

  it("copies a built-in agent's instructions to a new name and saves it with the usual PUT", async () => {
    show("/agents", routes);
    await newAgent();
    fireEvent.click(await screen.findByRole("button", { name: "From an existing agent" }));
    // The Coder is the first agent on offer, and is the one chosen (which agents are offered is `copyable`'s test).
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByDisplayValue("You are the Coder.")).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Name of the new agent"), {
      target: { value: "Quick" },
    });
    fireEvent.change(screen.getByLabelText("Model"), { target: { value: "qwen-local" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await vi.waitFor(() => expect(writes).toHaveLength(1));
    expect(writes[0]?.[0]).toBe("PUT");
    expect(writes[0]?.[1]).toBe("/v1/agents/Quick");
    expect(writes[0]?.[2]).toMatchObject({
      // The copy has the Coder's engineering skills as a list of its own, since its name is another.
      settings: {
        model: "qwen-local",
        engineering_skills: ["tdd", "diagnosing-bugs", "moebius-autonomous"],
      },
      instructions: "You are the Coder.",
    });
  });

  it("copies an agent's settings and refuses a name that is taken", async () => {
    show("/agents?new=1&from=Fast", routes);
    expect(await screen.findByDisplayValue("small")).toBeTruthy();
    expect(screen.getByDisplayValue("Be brief.")).toBeTruthy();
    const name = screen.getByLabelText("Name of the new agent");
    fireEvent.change(name, { target: { value: "Fast" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText(/An agent named Fast exists/)).toBeTruthy();
    expect(writes).toEqual([]);
    // The built-in Coder is taken too, though it has no files.
    fireEvent.change(name, { target: { value: "Coder" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText(/An agent named Coder exists/)).toBeTruthy();
    expect(writes).toEqual([]);
  });
});
