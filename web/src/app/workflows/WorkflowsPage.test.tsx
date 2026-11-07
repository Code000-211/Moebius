/**
 * The Workflows list: what each card says about a workflow and its last run, the link that opens the definition and the
 * link that opens the last run, and the sentence an empty list gives instead of a blank page.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { WorkflowsPage } from "./WorkflowsPage";

const LIST = [
  {
    name: "release",
    description: "Build, test and publish a release.",
    trigger: { kind: "schedule", detail: "0 6 * * 1" },
    node_count: 3,
    last_run: { id: 41, status: "failed", started_at: 1, finished_at: 2 },
  },
  {
    name: "triage",
    description: "Sort new issues.",
    trigger: { kind: "manual", detail: null },
    node_count: 1,
    last_run: null,
  },
];

function show(list: unknown) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => ({ ok: true, status: 200, text: async () => JSON.stringify(list) })),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <WorkflowsPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

afterEach(() => vi.unstubAllGlobals());

describe("the workflows list", () => {
  it("says what each workflow is, what starts it, its nodes and its last run", async () => {
    show(LIST);
    expect(await screen.findByRole("heading", { name: "release" })).toBeTruthy();
    expect(screen.getByText("Build, test and publish a release.")).toBeTruthy();
    expect(screen.getByText("Schedule · 0 6 * * 1")).toBeTruthy();
    expect(screen.getByText("3 nodes")).toBeTruthy();
    expect(screen.getByRole("link", { name: /Failed/ })).toBeTruthy();

    expect(screen.getByRole("heading", { name: "triage" })).toBeTruthy();
    expect(screen.getByText("Manual")).toBeTruthy();
    expect(screen.getByText("Never run")).toBeTruthy();
  });

  it("opens the definition by its name, escaping the name in the address", async () => {
    show([{ ...LIST[0], name: "night build" }]);
    const link = await screen.findByRole("link", { name: "night build" });
    expect(link.getAttribute("href")).toBe("/workflows/night%20build");
  });

  it("says there is no workflow rather than drawing an empty list", async () => {
    show([]);
    expect(await screen.findByText("No workflow yet")).toBeTruthy();
  });
});
