/**
 * The definition view: the graph at the saved positions with each node's kind and parameters, the trigger, and the raw
 * file with its copy button. The nodes are found by the words a person reads, not by a test-only handle on the canvas.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { WorkflowPage } from "./WorkflowPage";

const DEFINITION = {
  name: "release",
  description: "Build, test and publish a release.",
  trigger: { kind: "schedule", detail: "0 6 * * 1" },
  node_count: 2,
  last_run: { id: 41, status: "failed", started_at: 1, finished_at: 2 },
  nodes: [
    {
      id: "build",
      kind: "agent",
      parameters: { prompt: "Build the release" },
      position: { x: 16, y: 16 },
    },
    {
      id: "test",
      kind: "script",
      parameters: { command: "cargo test --workspace" },
      position: { x: 320, y: 16 },
    },
  ],
  edges: [{ from: "build", to: "test" }],
  file: 'name = "release"\ntrigger = { kind = "schedule" }\n',
};

function show(definition: unknown) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => ({
      ok: true,
      status: 200,
      text: async () => JSON.stringify(definition),
    })),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={["/workflows/release"]}>
        <Routes>
          <Route path="/workflows/:name" element={<WorkflowPage />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

afterEach(() => vi.unstubAllGlobals());

describe("a workflow's definition", () => {
  it("draws every saved node with its kind and parameters", async () => {
    show(DEFINITION);
    const graph = await screen.findByRole("region", { name: "Definition graph" });
    expect(within(graph).getByRole("heading", { name: "build" })).toBeTruthy();
    expect(within(graph).getByText("agent")).toBeTruthy();
    expect(within(graph).getByText(/Build the release/)).toBeTruthy();
    expect(within(graph).getByRole("heading", { name: "test" })).toBeTruthy();
    expect(within(graph).getByText("script")).toBeTruthy();
    expect(within(graph).getByText(/cargo test --workspace/)).toBeTruthy();
  });

  it("shows the file as it is saved, with the way to copy it", async () => {
    show(DEFINITION);
    expect(await screen.findByRole("heading", { name: "File" })).toBeTruthy();
    expect(screen.getByText(/name = "release"/)).toBeTruthy();
    expect(screen.getByRole("button", { name: /Copy/ })).toBeTruthy();
  });

  it("says what starts it and how the last run went", async () => {
    show(DEFINITION);
    expect(await screen.findByText("Schedule · 0 6 * * 1")).toBeTruthy();
    expect(screen.getByText(/Last run: failed/)).toBeTruthy();
    expect(screen.getByRole("link", { name: /Open last run/ }).getAttribute("href")).toBe(
      "/workflow-runs/41",
    );
  });

  it("names a successful last run as done, as the list does", async () => {
    show({ ...DEFINITION, last_run: { ...DEFINITION.last_run, status: "succeeded" } });
    expect(await screen.findByText(/Last run: done/)).toBeTruthy();
  });
});

/** An automation as the Automations page holds one: a schedule that starts one chat. */
const AUTOMATION = {
  name: "tidy",
  description: "Tidy old branches.",
  trigger: { kind: "schedule", detail: "*/5 * * * *" },
  node_count: 1,
  last_run: null,
  nodes: [
    {
      id: "chat",
      kind: "chat",
      parameters: { prompt: "Tidy the old branches" },
      position: { x: 16, y: 16 },
    },
  ],
  edges: [],
  file: 'name = "tidy"\n',
};

interface DeleteAnswer {
  status: number;
  body: string;
}

/** Shows the detail page of `definition`, answering `DELETE /v1/workflows/{name}` with `answer`. */
function showDeleting(definition: unknown, answer?: DeleteAnswer) {
  const calls: Array<{ method: string; url: string }> = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const method = init?.method ?? "GET";
      const url = String(input);
      calls.push({ method, url });
      if (method === "DELETE") {
        const gave = answer ?? {
          status: 200,
          body: JSON.stringify({ name: "x", commit: "abc1234" }),
        };
        return {
          ok: gave.status >= 200 && gave.status < 300,
          status: gave.status,
          text: async () => gave.body,
        };
      }
      return { ok: true, status: 200, text: async () => JSON.stringify(definition) };
    }),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={["/workflows/release"]}>
        <Routes>
          <Route path="/workflows/:name" element={<WorkflowPage />} />
          <Route path="/workflows" element={<p>the list of workflows</p>} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
  return calls;
}

const openDeleteDialog = async (name: string) => {
  await screen.findByRole("button", { name: "Delete workflow" });
  fireEvent.click(screen.getByRole("button", { name: "Delete workflow" }));
  return screen.getByRole("dialog", { name: `Delete ${name}?` });
};

describe("deleting a workflow from its detail page", () => {
  it("asks first, and warns that a workflow shaped like an automation stops one", async () => {
    showDeleting(AUTOMATION);
    const dialog = await openDeleteDialog("tidy");
    expect(within(dialog).getByText(/automation: a schedule that starts one chat/)).toBeTruthy();
    fireEvent.click(within(dialog).getByRole("button", { name: "Keep it" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("says no word about automations for a plain workflow", async () => {
    showDeleting(DEFINITION);
    const dialog = await openDeleteDialog("release");
    expect(within(dialog).queryByText(/stops the automation/)).toBeNull();
  });

  it("deletes and returns to the list of workflows", async () => {
    const calls = showDeleting(DEFINITION);
    const dialog = await openDeleteDialog("release");
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    expect(await screen.findByText("the list of workflows")).toBeTruthy();
    expect(calls).toContainEqual({ method: "DELETE", url: "/v1/workflows/release" });
  });

  it("says the refusal of a run still queued or running, and stays", async () => {
    showDeleting(DEFINITION, {
      status: 409,
      body: "workflow release has runs queued, running or waiting",
    });
    const dialog = await openDeleteDialog("release");
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    expect(await screen.findByText(/has runs queued, running or waiting/)).toBeTruthy();
    expect(screen.queryByText("the list of workflows")).toBeNull();
  });
});
