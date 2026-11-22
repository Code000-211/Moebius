// @vitest-environment jsdom
/**
 * The Workflows tab of a project: only this project's workflows, runs and scheduled workflows; "Run a workflow here"
 * sends the project and one of the project's repositories and builds its form from `run-inputs`; the Scheduled filter
 * lists the schedule-trigger workflows with when each fires next; "New scheduled workflow" saves an automation of the
 * project from the template.
 */
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createMemoryRouter, RouterProvider, useLocation } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Project } from "../../api/types/projects";
import type { WorkflowSummary } from "../../api/types/workflows";
import { WorkflowsTab } from "./WorkflowsTab";

const NOW = Math.floor(Date.now() / 1000);

const project = (id: number, name: string, repositories: Project["repositories"]): Project =>
  ({
    id,
    name,
    key: name.slice(0, 3).toUpperCase(),
    created_at: id,
    private: false,
    repositories,
    members: [],
    open_cards: 0,
    files: null,
    kinds: [],
    intake: false,
    member: true,
  }) as Project;

const PLATFORM = project(1, "Platform", [
  { id: 5, name: "platform-api", home: true },
  { id: 6, name: "platform-web", home: false },
]);
const BLOG = project(2, "Blog", [{ id: 7, name: "blog", home: true }]);

const summary = (
  projectId: number,
  name: string,
  trigger: string,
  last: string | null,
  extra: Partial<WorkflowSummary> = {},
): WorkflowSummary => ({
  project: projectId,
  project_name: projectId === 1 ? "Platform" : "Blog",
  name,
  description: `${name} does its work.`,
  trigger_kind: trigger,
  node_count: 2,
  last_run:
    last === null
      ? null
      : { id: 1, status: last, created_at: NOW, started_at: NOW, finished_at: null },
  ...extra,
});

const run = (
  id: number,
  projectId: number,
  workflow: string,
  repository: number,
  status: string,
  trigger = "manual",
  error: string | null = null,
) => ({
  id,
  workflow,
  project: projectId,
  project_name: projectId === 1 ? "Platform" : "Blog",
  repository_id: repository,
  trigger,
  status,
  value: null,
  created_at: NOW - 60,
  started_at: NOW - 60,
  finished_at: null,
  error,
});

const WORKFLOWS: Record<number, WorkflowSummary[]> = {
  1: [
    summary(1, "release", "manual", "failed"),
    summary(1, "triage", "schedule", "succeeded"),
    summary(1, "on-push", "webhook", null, { hook_address: "/api/hooks/projects/1/push" }),
    summary(1, "nightly-sweep", "schedule", null, {
      next_firing: NOW + 5 * 3600 + 60,
      last_fired: NOW - 19 * 3600,
    }),
    summary(1, "paused-sweep", "schedule", null),
  ],
  2: [
    summary(2, "blog-deploy", "manual", null),
    summary(2, "blog-hook", "webhook", null, { hook_address: "/api/hooks/projects/2/blog" }),
  ],
};
const RUNS = [
  run(11, 1, "release", 5, "running"),
  run(12, 1, "triage", 5, "succeeded", "schedule"),
  run(13, 1, "release", 6, "failed", "manual", 'failed at "deploy"'),
  run(21, 2, "blog-deploy", 7, "succeeded"),
];
const AUTOMATIONS = [
  {
    id: 3,
    name: "triage",
    schedule: "0 8 * * *",
    agent: "Moebius",
    prompt: "Sort the new issues.",
    repository_id: 5,
    project_id: 1,
    harness_repository: false,
    enabled: true,
    next_firing: NOW + 3 * 3600 + 60,
    last_firing: null,
  },
  {
    id: 4,
    name: "blog-digest",
    schedule: "0 9 * * 5",
    agent: "Moebius",
    prompt: "Digest.",
    repository_id: 7,
    project_id: 2,
    harness_repository: false,
    enabled: true,
    next_firing: NOW + 86_400 * 2 + 60,
    last_firing: null,
  },
];

interface Call {
  method: string;
  url: string;
  body: unknown;
}
let calls: Call[];
let inputs: unknown[];
/** What the server refuses a started run with, when a test makes it. */
let refusal: string | null;

function answer(body: unknown, status = 200) {
  return { ok: status < 300, status, text: async () => JSON.stringify(body) };
}

/** Opens the menu of the choice named `name` in `scope` and answers its rows, the way a person opens it. */
async function open(scope: HTMLElement, name: RegExp): Promise<HTMLElement[]> {
  fireEvent.pointerDown(within(scope).getByRole("button", { name }), {
    button: 0,
    ctrlKey: false,
  });
  return screen.findAllByRole("menuitemradio");
}
const words = (rows: HTMLElement[]) => rows.map((each) => each.textContent?.trim());

function Where() {
  const location = useLocation();
  return <output data-testid="where">{location.pathname + location.search}</output>;
}

beforeEach(() => {
  calls = [];
  inputs = [];
  refusal = null;
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      const method = init?.method ?? "GET";
      const body = typeof init?.body === "string" ? JSON.parse(init.body) : undefined;
      calls.push({ method, url, body });
      const listed = /^\/v1\/projects\/(\d+)\/workflows$/.exec(url);
      if (listed) return answer(WORKFLOWS[Number(listed[1])] ?? []);
      const asked = /^\/v1\/workflow-runs\?project=(\d+)$/.exec(url);
      if (asked) return answer(RUNS.filter((each) => each.project === Number(asked[1])));
      if (url === "/v1/automations" && method === "GET") return answer(AUTOMATIONS);
      if (url === "/v1/automations" && method === "POST") return answer({ ...AUTOMATIONS[0] });
      if (url.endsWith("/release/run-inputs")) return answer({ trigger_kind: "manual", inputs });
      if (url.endsWith("/runs") && method === "POST" && refusal !== null)
        return { ok: false, status: 422, text: async () => refusal };
      if (url.endsWith("/runs") && method === "POST")
        return answer({
          id: 99,
          workflow: "release",
          repository: "x",
          trigger: "manual",
          status: "queued",
        });
      if (url === "/v1/agents") return answer([{ name: "Moebius" }]);
      throw new Error(`unexpected call ${method} ${url}`);
    }),
  );
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function show(target: Project, at = `/projects/${target.id}/workflows`) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const router = createMemoryRouter(
    [
      {
        path: "/projects/:projectId/workflows",
        element: (
          <>
            <WorkflowsTab project={target} onTerminal={() => {}} />
            <Where />
          </>
        ),
      },
      { path: "/workflow-runs/:id", element: <p data-testid="run-page">a run</p> },
      { path: "/projects/:projectId/workflows/:name/canvas", element: <Where /> },
    ],
    { initialEntries: [at] },
  );
  render(
    <QueryClientProvider client={client}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return router;
}

describe("the tab's lists", () => {
  it("shows the workflows and runs of this project only", async () => {
    show(PLATFORM);
    const rows = await screen.findAllByTestId("workflow-row");
    expect(rows.map((row) => within(row).getByRole("heading").textContent)).toEqual([
      "release",
      "triage",
      "on-push",
      "nightly-sweep",
      "paused-sweep",
    ]);
    expect(screen.queryByText("blog-deploy")).toBeNull();
    const runs = screen.getAllByTestId("run-row");
    expect(runs).toHaveLength(3);
    expect(runs[0]?.textContent).toContain("platform-api · running");
    expect(runs[2]?.textContent).toContain('platform-web · failed · failed at "deploy"');
    const asked = calls.map((call) => call.url);
    expect(asked).toContain("/v1/projects/1/workflows");
    expect(asked).toContain("/v1/workflow-runs?project=1");
    expect(asked.some((url) => url.includes("/projects/2/") || url.endsWith("project=2"))).toBe(
      false,
    );
  });

  it("shows the other project's own when that project is open", async () => {
    show(BLOG);
    const rows = await screen.findAllByTestId("workflow-row");
    expect(rows).toHaveLength(2);
    expect(screen.getAllByTestId("run-row")).toHaveLength(1);
    expect(screen.queryByText("release")).toBeNull();
    // Its own hook shows, with its own project in the address, and the first project's does not.
    expect(screen.getByTestId("hook-address").textContent).toBe("/api/hooks/projects/2/blog");
  });

  it("links a name to its definition under the project and a run to its page", async () => {
    show(PLATFORM);
    const link = await screen.findByRole("link", { name: "release" });
    expect(link.getAttribute("href")).toBe("/projects/1/workflows/release");
    expect(screen.getAllByTestId("run-row")[0]?.querySelector("a")?.getAttribute("href")).toBe(
      "/workflow-runs/11",
    );
  });

  it("filters the runs to the failed ones", async () => {
    show(PLATFORM);
    await screen.findAllByTestId("run-row");
    fireEvent.click(screen.getByRole("tab", { name: "Failed" }));
    const runs = await screen.findAllByTestId("run-row");
    expect(runs).toHaveLength(1);
    expect(runs[0]?.textContent).toContain("release");
    expect(screen.getByTestId("where").textContent).toBe("/projects/1/workflows?runs=failed");
  });

  it("makes a new workflow's canvas open under the project", async () => {
    show(PLATFORM);
    fireEvent.click(await screen.findByRole("button", { name: "New workflow" }));
    const dialog = await screen.findByRole("dialog", { name: "New workflow" });
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "nightly" } });
    fireEvent.click(within(dialog).getByTestId("new-workflow-create"));
    await waitFor(() =>
      expect(screen.getByTestId("where").textContent).toBe("/projects/1/workflows/nightly/canvas"),
    );
  });
});

describe("the Scheduled filter", () => {
  it("lists the schedule-trigger workflows with when each fires next, and the scheduled runs", async () => {
    show(PLATFORM, "/projects/1/workflows?runs=scheduled");
    const rows = await screen.findAllByTestId("scheduled-row");
    expect(rows).toHaveLength(3);
    expect(rows[0]?.textContent).toContain("triage");
    expect(rows[0]?.textContent).toContain("Every morning at 8 · next in 3 h");
    // A schedule that is not an automation says when it fires next too, as the server worked it out.
    const sweep = rows.find((each) => each.textContent?.includes("nightly-sweep"));
    expect(sweep?.textContent).toContain("Schedule · next in 5 h · last fired 19h ago");
    const paused = rows.find((each) => each.textContent?.includes("paused-sweep"));
    expect(paused?.textContent).toContain("Schedule · no next firing");
    // The other project's automation is not here.
    expect(screen.queryByText("blog-digest")).toBeNull();
    const runs = screen.getAllByTestId("run-row");
    expect(runs).toHaveLength(1);
    expect(runs[0]?.textContent).toContain("triage");
  });

  it("edits an automation in the window, which is in the address", async () => {
    show(PLATFORM, "/projects/1/workflows?runs=scheduled");
    fireEvent.click(await screen.findByRole("button", { name: "Edit scheduled workflow triage" }));
    expect(
      await screen.findByRole("dialog", { name: "Edit scheduled workflow triage" }),
    ).toBeTruthy();
    expect(screen.getByTestId("where").textContent).toBe(
      "/projects/1/workflows?runs=scheduled&automation=3",
    );
  });

  it("creates a scheduled workflow of this project from the template", async () => {
    show(PLATFORM);
    fireEvent.click(await screen.findByRole("button", { name: "New scheduled workflow" }));
    const dialog = await screen.findByRole("dialog", { name: "New scheduled workflow" });
    // Where its chats work is one of this project's repositories.
    const rows = await open(dialog, /^Works in/);
    expect(words(rows)).toEqual([
      "An empty directory",
      "platform-api",
      "platform-web",
      "Harness configuration",
    ]);
    fireEvent.click(rows[2] as HTMLElement);
    fireEvent.change(within(dialog).getByLabelText("Name"), { target: { value: "weekly" } });
    fireEvent.change(within(dialog).getByLabelText("Prompt"), { target: { value: "Summarise." } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(calls.some((call) => call.method === "POST")).toBe(true));
    const saved = calls.find((call) => call.method === "POST" && call.url === "/v1/automations");
    expect(saved?.body).toMatchObject({
      name: "weekly",
      project: 1,
      repository: 6,
      prompt: "Summarise.",
    });
  });
});

describe("Run a workflow here", () => {
  it("lists only the project's repositories, the home one chosen, and sends it with the run", async () => {
    show(PLATFORM);
    fireEvent.click(await screen.findByRole("button", { name: "Run a workflow here" }));
    const dialog = await screen.findByRole("dialog", { name: "Run a workflow in Platform" });
    // Only the manual workflows of the project are offered.
    await within(dialog).findByRole("button", { name: /^Workflow/ });
    expect(words(await open(dialog, /^Workflow/))).toEqual(["release"]);
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
    const rows = await open(dialog, /^Repository/);
    expect(words(rows)).toEqual(["platform-api", "platform-web"]);
    expect(rows.map((each) => each.getAttribute("aria-checked"))).toEqual(["true", "false"]);
    fireEvent.click(rows[1] as HTMLElement);
    fireEvent.click(within(dialog).getByRole("button", { name: "Run" }));
    await waitFor(() => expect(screen.getByTestId("run-page")).toBeTruthy());
    const started = calls.find((call) => call.method === "POST");
    expect(started?.url).toBe("/v1/projects/1/workflows/release/runs");
    expect(started?.body).toEqual({ repository: "platform-web", inputs: {} });
  });

  it("builds the form from the inputs the manual trigger names", async () => {
    inputs = [
      { name: "version", type: "text", required: true },
      { name: "attempts", type: "number", required: false, default: 3 },
      { name: "channel", type: "choice", required: false, choices: ["stable", "beta"] },
      { name: "dry", type: "boolean", required: false },
      { name: "target", type: "repository", required: false },
    ];
    show(PLATFORM);
    fireEvent.click(await screen.findByRole("button", { name: "Run a workflow here" }));
    const dialog = await screen.findByRole("dialog", { name: "Run a workflow in Platform" });
    fireEvent.change(await within(dialog).findByLabelText("version (required)"), {
      target: { value: "1.2" },
    });
    fireEvent.change(within(dialog).getByLabelText("attempts"), { target: { value: "5" } });
    fireEvent.click((await open(dialog, /^channel/))[2] as HTMLElement);
    fireEvent.click((await open(dialog, /^dry/))[1] as HTMLElement);
    // A repository input is a choice among the project's repositories, so the dialog asks for no other one.
    const target = await open(dialog, /^target/);
    expect(words(target)).toEqual(["Not set", "platform-api", "platform-web"]);
    expect(within(dialog).queryByRole("button", { name: /^Repository/ })).toBeNull();
    fireEvent.click(target[2] as HTMLElement);
    fireEvent.click(within(dialog).getByRole("button", { name: "Run" }));
    await waitFor(() => expect(calls.some((call) => call.method === "POST")).toBe(true));
    expect(calls.find((call) => call.method === "POST")?.body).toEqual({
      inputs: { version: "1.2", attempts: 5, channel: "beta", dry: true, target: "platform-web" },
    });
  });

  it("says the refusal of the server in the dialog", async () => {
    refusal = "the repository platform-web is not one of the project's";
    show(PLATFORM);
    fireEvent.click(await screen.findByRole("button", { name: "Run a workflow here" }));
    const dialog = await screen.findByRole("dialog", { name: "Run a workflow in Platform" });
    await within(dialog).findByLabelText("Repository");
    fireEvent.click(within(dialog).getByRole("button", { name: "Run" }));
    expect(await within(dialog).findByText(/is not one of the project's/)).toBeTruthy();
  });
});

describe("the address of a webhook workflow", () => {
  it("names the project, on the workflow's own row", async () => {
    show(PLATFORM);
    const rows = await screen.findAllByTestId("workflow-row");
    const hooked = rows.find((each) => each.textContent?.includes("on-push"));
    expect(within(hooked as HTMLElement).getByTestId("hook-address").textContent).toBe(
      "/api/hooks/projects/1/push",
    );
    // Another trigger has no address, and another project's hook is not here.
    expect(screen.getAllByTestId("hook-address")).toHaveLength(1);
    expect(screen.queryByText("/api/hooks/projects/2/blog")).toBeNull();
    expect(screen.queryByText("blog-hook")).toBeNull();
  });

  it("copies the address", async () => {
    const writeText = vi.fn(async () => {});
    vi.stubGlobal("navigator", { ...navigator, clipboard: { writeText } });
    show(PLATFORM);
    fireEvent.click(
      await screen.findByRole("button", { name: "Copy the webhook address of on-push" }),
    );
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("/api/hooks/projects/1/push"));
    expect(await screen.findByText("Copied")).toBeTruthy();
  });
});
