/**
 * A run's page: the status and the trigger, a panel for each node with its input, output, log and error, the redaction
 * mark kept as the string the server sent, a long value cut until it is asked for whole, and a waiting run saying where
 * it stopped and what it asks.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { WorkflowRun } from "../../api/workflows";
import { WorkflowRunPage } from "./WorkflowRunPage";

const LONG = "the workspace compiled and the tests began to run\n".repeat(40);

const FAILED: WorkflowRun = {
  id: 41,
  workflow: "release",
  repository_id: 1,
  trigger: { kind: "schedule", detail: "0 6 * * 1" },
  status: "failed",
  value: null,
  created_at: 1,
  started_at: 1,
  finished_at: 95,
  error: "The test node failed.",
  nodes: [
    {
      node: "build",
      kind: "agent",
      attempt: 1,
      status: "succeeded",
      input: { prompt: "Build the release" },
      output: { stdout: LONG },
      log: "built the workspace in 41s",
      error: null,
      started_at: 1,
      finished_at: 42,
    },
    {
      node: "test",
      kind: "script",
      attempt: 1,
      status: "failed",
      input: { command: "cargo test", token: "[redacted]" },
      output: null,
      log: "61 passed; 1 failed",
      error: "expected `done`, got `in progress`",
      started_at: 42,
      finished_at: 95,
    },
  ],
  waiting: null,
};

const WAITING: WorkflowRun = {
  ...FAILED,
  id: 42,
  workflow: "deploy",
  trigger: { kind: "manual", detail: null },
  status: "waiting",
  error: null,
  nodes: [
    ...FAILED.nodes.slice(0, 1),
    {
      node: "approve",
      kind: "wait",
      attempt: 1,
      status: "waiting",
      input: { version: "1.4.0" },
      output: null,
      log: "waiting for a person",
      error: null,
      started_at: 42,
      finished_at: null,
    },
  ],
  waiting: {
    node: "approve",
    asks: { question: "Publish 1.4.0 to the registry?", token: "[redacted]" },
  },
};

function show(run: WorkflowRun) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => ({ ok: true, status: 200, text: async () => JSON.stringify(run) })),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[`/workflow-runs/${run.id}`]}>
        <Routes>
          <Route path="/workflow-runs/:id" element={<WorkflowRunPage />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

afterEach(() => vi.unstubAllGlobals());

describe("a run that failed at a node", () => {
  it("says the status, the trigger and why it failed", async () => {
    show(FAILED);
    expect((await screen.findAllByText("Failed")).length).toBeGreaterThan(0);
    expect(screen.getByText("Schedule · 0 6 * * 1")).toBeTruthy();
    expect(screen.getByText("The test node failed.")).toBeTruthy();
  });

  it("gives each node its input, output, log and error", async () => {
    show(FAILED);
    const test = await screen.findByRole("region", { name: "Node test" });
    expect(within(test).getByText(/cargo test/)).toBeTruthy();
    expect(within(test).getByText("61 passed; 1 failed")).toBeTruthy();
    expect(within(test).getByText(/expected `done`/)).toBeTruthy();
  });

  it("keeps a redacted value as the mark the server sent", async () => {
    show(FAILED);
    const test = await screen.findByRole("region", { name: "Node test" });
    expect(within(test).getByText(/\[redacted\]/)).toBeTruthy();
  });

  it("cuts a long value and shows all of it when asked", async () => {
    show(FAILED);
    const build = await screen.findByRole("region", { name: "Node build" });
    const value = build.querySelector('[data-json="build-output"]');
    expect(value?.getAttribute("data-complete")).toBe("false");
    expect(value?.textContent).toContain("…");
    fireEvent.click(within(build).getByRole("button", { name: /Show all/ }));
    expect(value?.getAttribute("data-complete")).toBe("true");
    expect(value?.textContent).not.toContain("…");
  });
});

describe("a run that waits", () => {
  it("says where it stopped and what it asks", async () => {
    show(WAITING);
    const waiting = await screen.findByRole("region", { name: "Waiting" });
    expect(within(waiting).getByRole("heading", { name: "Waiting at approve" })).toBeTruthy();
    expect(within(waiting).getByText(/Publish 1.4.0 to the registry\?/)).toBeTruthy();
  });

  it("marks the node it stopped on", async () => {
    show(WAITING);
    const approve = await screen.findByRole("region", { name: "Node approve" });
    expect(approve.getAttribute("aria-current")).toBe("step");
  });
});
