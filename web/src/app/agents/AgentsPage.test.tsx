/**
 * The draft of an agent on the Agents page: choosing another row keeps what was typed, and the editor that reopens on
 * that draft still says the work is waiting — what the file holds did not change, so there is something to save.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
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

describe("the draft of an agent", () => {
  it("survives another row being chosen, and still says it is unsaved", async () => {
    show();
    const model = await screen.findByLabelText("Model");
    fireEvent.change(model, { target: { value: "qwen-local" } });
    expect(await screen.findByText("Unsaved changes")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: /Reviewer/ }));
    expect(await screen.findByDisplayValue("Say what is wrong.")).toBeTruthy();
    // Nothing to write is said by the quiet button, not by a warning beside one that still works.
    expect(saveButton().disabled).toBe(true);
    expect(screen.queryByText("Nothing to save")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: /Coder/ }));
    expect(await screen.findByDisplayValue("qwen-local")).toBeTruthy();
    // The file still has `fake-small`, so the draft is work waiting for a save, not nothing to save.
    expect(await screen.findByText("Unsaved changes")).toBeTruthy();
    expect(saveButton().disabled).toBe(false);
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
