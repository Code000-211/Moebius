// @vitest-environment jsdom
/**
 * One workflow's page: the header (the way back, the name, what starts it, the tabs, Copy hook URL and Run), the
 * Overview with its steps in plain words, the Runs and File tabs, the Canvas tab opening, and Delete. The page is
 * read by the words a person reads, not by a handle made for tests.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createMemoryRouter, RouterProvider, useLocation } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import fixtures from "../../api/fixtures/workflows.json";
import { WorkflowPage } from "./WorkflowPage";

const RUNS = fixtures.runs.map((run) => ({ ...run, status: "failed" }));

const PROMPT = `Build the release.

Then read {{ input.summary }} and write the notes:
- group the changes
- name the breaking ones
${"Keep every note short and plain. ".repeat(12)}`;

/** What `GET /v1/projects/{id}/workflows/{name}` answers, with a prompt of many lines and expressions. */
const DEFINITION = {
  ...fixtures.show,
  file: 'name = "release"\ntrigger = { kind = "schedule" }\n',
  definition: {
    ...fixtures.show.definition,
    trigger: [{ kind: "schedule", params: { cron: "0 6 * * 1" } }],
    nodes: [
      {
        id: "test",
        kind: "http",
        params: { url: "https://ci.example.test/{{ input.id }}", method: "post" },
        position: { x: 320, y: 16 },
      },
      {
        id: "build",
        kind: "agent",
        params: { agent: "Coder", prompt: PROMPT, output: "summary" },
        position: { x: 16, y: 16 },
      },
    ],
    edges: [
      { from: "trigger", to: "build", port: "out" },
      { from: "build", to: "test", port: "out" },
    ],
  },
};

const NEXT = 4_000_000_000;
/** The project's list: where the next firing and the hook's address come from. */
const LIST = [
  {
    ...fixtures.list[0],
    name: "release",
    trigger_kind: "schedule",
    next_firing: NEXT,
    last_fired: 1700000000,
  },
];

/** The project the workflow belongs to, as the list of projects answers it. */
const PROJECTS = [
  {
    id: 1,
    name: "demo",
    key: "DEMO",
    created_at: 1,
    repositories: [{ id: 5, name: "platform-api", home: true }],
    members: [],
    open_cards: 0,
    files: null,
    kinds: [],
    intake: false,
    private: false,
    member: true,
  },
];

/** The routes of the page: the projects, the project's workflows, the runs, then the workflow. */
function routed(definition: unknown, runs: unknown = RUNS, list: unknown = LIST) {
  return (url: string) =>
    url === "/v1/projects"
      ? PROJECTS
      : url === "/v1/projects/1/workflows"
        ? list
        : url.includes("/v1/workflow-runs")
          ? runs
          : definition;
}

/** Where the router is, so a test can read the address a press led to. */
function Where() {
  return <p data-testid="where">{useLocation().pathname}</p>;
}

function show(
  definition: unknown,
  runs: unknown = RUNS,
  path = "/projects/1/workflows/release",
  list: unknown = LIST,
) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => ({
      ok: true,
      status: 200,
      text: async () => JSON.stringify(routed(definition, runs, list)(String(input))),
    })),
  );
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <RouterProvider router={routerAt(path)} />
    </QueryClientProvider>,
  );
}

/** A data router, which `useBlocker` — the canvas's ask-before-leaving — needs. */
function routerAt(path: string) {
  return createMemoryRouter(
    [
      {
        path: "/projects/:projectId/workflows/:name/:tab?",
        element: (
          <>
            <Where />
            <WorkflowPage />
          </>
        ),
      },
      { path: "/projects/:projectId/workflows", element: <p>the list of workflows</p> },
      { path: "/projects/:projectId", element: <p>the board</p> },
    ],
    { initialEntries: [path] },
  );
}

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("the header of a workflow's page", () => {
  it("says where it is, what starts it and when a schedule fires next, and has the four tabs", async () => {
    show(DEFINITION);
    expect(await screen.findByRole("heading", { name: "release", level: 1 })).toBeTruthy();
    const trail = screen.getByRole("navigation", { name: "Where this is" });
    expect((await within(trail).findByRole("link", { name: "demo" })).getAttribute("href")).toBe(
      "/projects/1",
    );
    expect(within(trail).getByRole("link", { name: "Workflows" }).getAttribute("href")).toBe(
      "/projects/1/workflows",
    );
    expect(screen.getAllByText(/^Schedule · next /).length).toBeGreaterThan(0);
    const tabs = within(screen.getByRole("tablist", { name: "Workflow" })).getAllByRole("tab");
    expect(tabs.map((tab) => tab.textContent)).toEqual(["Overview", "Canvas", "Runs", "File"]);
    expect(screen.getByRole("tab", { name: "Overview" }).getAttribute("aria-selected")).toBe(
      "true",
    );
  });

  it("offers Copy hook URL only for a webhook, copying the project's address", async () => {
    show(DEFINITION);
    await screen.findByRole("heading", { name: "release", level: 1 });
    expect(screen.queryByRole("button", { name: "Copy hook URL" })).toBeNull();
    cleanup();

    const writeText = vi.fn(async () => {});
    vi.stubGlobal("navigator", { ...navigator, clipboard: { writeText } });
    const webhook = {
      ...DEFINITION,
      definition: {
        ...DEFINITION.definition,
        trigger: [{ kind: "webhook", params: { path: "notify" } }],
      },
    };
    const list = [
      { ...LIST[0], trigger_kind: "webhook", hook_address: "/api/hooks/projects/1/notify" },
    ];
    show(webhook, RUNS, "/projects/1/workflows/release", list);
    expect(await screen.findAllByText("Webhook")).not.toHaveLength(0);
    fireEvent.click(await screen.findByRole("button", { name: "Copy hook URL" }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("/api/hooks/projects/1/notify"));
    expect(await screen.findByText("Copied")).toBeTruthy();
  });

  it("goes to a tab's address when it is chosen, and reads the tab from the address", async () => {
    show(DEFINITION);
    fireEvent.click(await screen.findByRole("tab", { name: "File" }));
    await waitFor(() =>
      expect(screen.getByTestId("where").textContent).toBe("/projects/1/workflows/release/file"),
    );
    expect(screen.getByRole("tab", { name: "File" }).getAttribute("aria-selected")).toBe("true");
    fireEvent.click(screen.getByRole("tab", { name: "Runs" }));
    await waitFor(() =>
      expect(screen.getByTestId("where").textContent).toBe("/projects/1/workflows/release/runs"),
    );
  });

  it("offers Run, which asks for the inputs of the trigger", async () => {
    show(DEFINITION);
    fireEvent.click(await screen.findByRole("button", { name: "Run" }));
    expect(await screen.findByRole("dialog", { name: "Run release" })).toBeTruthy();
  });
});

describe("the Overview", () => {
  it("lists the steps in graph order, numbered, in plain words", async () => {
    show(DEFINITION);
    const steps = await screen.findAllByTestId("workflow-step");
    // The file lists `test` first; the graph runs `build` first.
    expect(steps.map((step) => step.querySelector("span.font-medium")?.textContent)).toEqual([
      "build",
      "test",
    ]);
    const build = steps[0] as HTMLElement;
    expect(within(build).getByText("1")).toBeTruthy();
    expect(within(build).getByText("Agent · Coder")).toBeTruthy();
    expect(within(build).getByText("summary")).toBeTruthy();
    expect(within(build).getByText("{{ input.summary }}").tagName).toBe("CODE");
    expect(within(steps[1] as HTMLElement).getByText("HTTP")).toBeTruthy();
    expect((steps[1] as HTMLElement).textContent).toContain(
      "POST https://ci.example.test/{{ input.id }}",
    );
  });

  it("cuts a long multi-line prompt to plain words, with no escaped newline and no JSON", async () => {
    show(DEFINITION);
    const [build] = await screen.findAllByTestId("workflow-step");
    const text = build?.textContent ?? "";
    expect(text).toContain("Build the release. Then read");
    expect(text).toContain("…");
    expect(text).not.toContain("\\n");
    expect(text).not.toMatch(/[{}[\]]"|":/);
    expect(document.body.textContent).not.toContain("\\n");
  });

  it("says the trigger with its schedule and next firing, and how the last run went", async () => {
    show(DEFINITION);
    expect(await screen.findByText("0 6 * * 1")).toBeTruthy();
    expect(await screen.findByText(/· next .*\(in /)).toBeTruthy();
    expect((await screen.findAllByText("Failed")).length).toBeGreaterThan(0);
    expect(screen.getByRole("link", { name: /Open run 7/ }).getAttribute("href")).toBe(
      "/workflow-runs/7",
    );
  });

  it("names a successful last run as done, as the list does", async () => {
    show(DEFINITION, [{ ...RUNS[0], status: "succeeded" }]);
    expect((await screen.findAllByText("Done")).length).toBeGreaterThan(0);
  });

  it("says a workflow that has not run, and one with no next firing", async () => {
    show(DEFINITION, [], "/projects/1/workflows/release", [{ ...LIST[0], next_firing: undefined }]);
    expect(
      await screen.findByText("This workflow has not run.", { selector: "span" }),
    ).toBeTruthy();
    expect(await screen.findByText(/no next firing/)).toBeTruthy();
  });

  it("links the last few runs to the Runs tab", async () => {
    show(DEFINITION);
    const link = await screen.findByRole("link", { name: "All runs" });
    expect(link.getAttribute("href")).toBe("/projects/1/workflows/release/runs");
  });
});

describe("the File tab", () => {
  it("shows the file as it is saved, with the way to copy it", async () => {
    show(DEFINITION, RUNS, "/projects/1/workflows/release/file");
    await waitFor(() => {
      expect(screen.getByTestId("workflow-file").querySelector("pre")?.textContent).toContain(
        'name = "release"',
      );
    });
    expect(screen.getByRole("button", { name: /Copy code/ })).toBeTruthy();
  });
});

describe("the Canvas tab", () => {
  it("opens the editor inside the page, under the same header", async () => {
    class Stub {
      observe() {}
      unobserve() {}
      disconnect() {}
    }
    window.ResizeObserver = Stub;
    show(DEFINITION, RUNS, "/projects/1/workflows/release/canvas");
    expect(await screen.findByTestId("canvas-page")).toBeTruthy();
    expect(screen.getByRole("heading", { name: "release", level: 1 })).toBeTruthy();
    expect(screen.getByRole("tab", { name: "Canvas" }).getAttribute("aria-selected")).toBe("true");
    expect(screen.getByTestId("canvas-state").textContent).toBe("Saved");
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

/** Shows the detail page of `definition`, answering `DELETE /v1/projects/{id}/workflows/{name}` with `answer`. */
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
      <RouterProvider router={routerAt("/projects/1/workflows/release")} />
    </QueryClientProvider>,
  );
  return calls;
}

const openDeleteDialog = async (name: string) => {
  // Radix opens a menu on the pointer going down, which jsdom has no mouse to send.
  const more = await screen.findByRole("button", { name: "More workflow actions" });
  fireEvent.pointerDown(more, { button: 0, ctrlKey: false, pointerType: "mouse" });
  fireEvent.click(await screen.findByRole("menuitem", { name: "Delete workflow" }));
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
    expect(calls).toContainEqual({ method: "DELETE", url: "/v1/projects/1/workflows/release" });
  });

  it("says the refusal of a run still queued or running, in the server's words, and stays", async () => {
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
        <RouterProvider router={routerAt("/projects/1/workflows/release/runs")} />
      </QueryClientProvider>,
    );
    const rows = await screen.findAllByTestId("workflow-run-row");
    expect(rows).toHaveLength(1);
    fireEvent.click(within(rows[0] as HTMLElement).getByRole("button", { name: "Cancel run 7" }));
    await waitFor(() =>
      expect(calls).toContainEqual({ method: "POST", url: "/v1/workflows/runs/7/cancel" }),
    );
  });

  it("reads only the runs of its project", async () => {
    show(DEFINITION);
    await screen.findByRole("heading", { name: "Runs" });
    const fetch = vi.mocked(globalThis.fetch);
    const asked = fetch.mock.calls.map((call) => String(call[0]));
    expect(asked).toContain("/v1/workflow-runs?project=1&workflow=release");
    expect(asked).toContain("/v1/projects/1/workflows/release");
  });
});
