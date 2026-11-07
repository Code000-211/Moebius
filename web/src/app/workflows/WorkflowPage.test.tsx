/**
 * The definition view: the graph at the saved positions with each node's kind and parameters, the trigger, and the raw
 * file with its copy button. The nodes are found by the words a person reads, not by a test-only handle on the canvas.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
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
    expect(screen.getByText(/Last run .*: failed\./)).toBeTruthy();
    expect(screen.getByRole("link", { name: /Open last run/ }).getAttribute("href")).toBe(
      "/workflow-runs/41",
    );
  });
});
