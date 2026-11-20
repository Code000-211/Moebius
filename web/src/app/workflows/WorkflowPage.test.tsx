/**
 * The definition view: the graph at the saved positions with each node's kind and parameters, the trigger, and the raw
 * file with its copy button. The nodes are found by the words a person reads, not by a test-only handle on the canvas.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import fixtures from "../../api/fixtures/workflows.json";
import { WorkflowPage } from "./WorkflowPage";

const RUNS = fixtures.runs.map((run) => ({ ...run, status: "failed" }));

/** What `GET /v1/workflows/{name}` answers, with the graph of `file`'s workflow: the real server's JSON, from the
 * shared fixtures. */
const DEFINITION = {
  ...fixtures.show,
  file: 'name = "release"\ntrigger = { kind = "schedule" }\n',
  definition: {
    ...fixtures.show.definition,
    trigger: [{ kind: "schedule", params: { cron: "0 6 * * 1" } }],
    nodes: [
      {
        id: "build",
        kind: "agent",
        params: { prompt: "Build the release" },
        position: { x: 16, y: 16 },
      },
      {
        id: "test",
        kind: "script",
        params: { command: "cargo test --workspace" },
        position: { x: 320, y: 16 },
      },
    ],
    edges: [
      { from: "trigger", to: "build", port: "out" },
      { from: "build", to: "test", port: "out" },
    ],
  },
};

/** The routes of the page: the workflow, then its runs. */
function routed(definition: unknown, runs: unknown = RUNS) {
  return (url: string) => (url.includes("/v1/workflow-runs") ? runs : definition);
}

function show(definition: unknown, runs: unknown = RUNS) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => ({
      ok: true,
      status: 200,
      text: async () => JSON.stringify(routed(definition, runs)(String(input))),
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
    // The file is plain text until the highlighter loads, then split into spans: read it as one text.
    await waitFor(() => {
      expect(document.querySelector("#workflow-file-title + p + div pre")?.textContent).toContain(
        'name = "release"',
      );
    });
    expect(screen.getByRole("button", { name: /Copy/ })).toBeTruthy();
  });

  it("says what starts it and how the last run went", async () => {
    show(DEFINITION);
    expect(await screen.findByText("Schedule")).toBeTruthy();
    expect(await screen.findByText(/Last run: failed/)).toBeTruthy();
    expect(screen.getByRole("link", { name: /Open last run/ }).getAttribute("href")).toBe(
      "/workflow-runs/7",
    );
  });

  it("names a successful last run as done, as the list does", async () => {
    show(DEFINITION, [{ ...RUNS[0], status: "succeeded" }]);
    expect(await screen.findByText(/Last run: done/)).toBeTruthy();
  });
});

/** An automation as the Automations page holds one: a schedule that starts one chat. */
const AUTOMATION = {
  ...DEFINITION,
  name: "tidy",
  file: 'name = "tidy"\n',
  definition: {
    ...DEFINITION.definition,
    name: "tidy",
    description: "Tidy old branches.",
    trigger: [{ kind: "schedule", params: { cron: "*/5 * * * *" } }],
    nodes: [
      {
        id: "chat",
        kind: "chat",
        params: { prompt: "Tidy the old branches" },
        position: { x: 16, y: 16 },
      },
    ],
    edges: [{ from: "trigger", to: "chat", port: "out" }],
  },
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
      return {
        ok: true,
        status: 200,
        text: async () => JSON.stringify(routed(definition)(url)),
      };
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

describe("the runs of a workflow", () => {
  it("lists them, and cancels one that has not ended", async () => {
    const calls: Array<{ method: string; url: string }> = [];
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
        const method = init?.method ?? "GET";
        const url = String(input);
        calls.push({ method, url });
        const body =
          method === "POST"
            ? RUNS[0]
            : routed(DEFINITION, [{ ...RUNS[0], status: "running" }])(url);
        return { ok: true, status: 200, text: async () => JSON.stringify(body) };
      }),
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
    const rows = await screen.findAllByTestId("workflow-run-row");
    expect(rows).toHaveLength(1);
    fireEvent.click(within(rows[0] as HTMLElement).getByRole("button", { name: "Cancel run 7" }));
    await waitFor(() =>
      expect(calls).toContainEqual({ method: "POST", url: "/v1/workflows/runs/7/cancel" }),
    );
  });

  it("offers Run, which asks for the inputs of the trigger", async () => {
    show(DEFINITION);
    fireEvent.click(await screen.findByRole("button", { name: "Run" }));
    expect(await screen.findByRole("dialog", { name: "Run release" })).toBeTruthy();
  });
});
