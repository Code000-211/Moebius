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
    { name: "Coder", model: "fake-small", protocol: "openai", harness: "pi-bolt" },
    { name: "Reviewer", model: "fake-small", protocol: "openai", harness: "pi-bolt" },
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

function show() {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (path: string) => ({
      ok: true,
      status: 200,
      text: async () => JSON.stringify(ROUTES[path.split("?")[0] ?? path] ?? []),
    })),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={["/agents?agent=Coder"]}>
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
