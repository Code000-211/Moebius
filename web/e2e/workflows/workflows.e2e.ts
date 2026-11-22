/**
 * The Workflows pages in a real browser: a project's tab with each workflow's trigger and last run, a definition's saved graph
 * and file, a run that failed at a node, and a run that waits. The failed run is moved on by the run and node events the
 * fake server broadcasts, which is how the page is checked to update without a reload. Most of it needs the fake's
 * workflows and runs and is skipped against a real server (MOEBIUS_E2E_SERVER); "a workflow saved through the API" at
 * the end runs against both, with a workflow it saves itself, so the pages are tried against the JSON the real server
 * answers and not only against the fake's.
 */
import { type Browser, type BrowserContext, chromium, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { startServer, type TestServer, usesRealServer } from "../server";

let browser: Browser;
let server: TestServer;
let context: BrowserContext;
let page: Page;

beforeAll(async () => {
  browser = await chromium.launch({
    executablePath: process.env.MOEBIUS_E2E_CHROMIUM || undefined,
  });
});

afterAll(async () => {
  await browser.close();
});

async function fresh() {
  server = await startServer();
  context = await browser.newContext({ viewport: { width: 1600, height: 1000 } });
  page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
}

afterEach(async () => {
  await context.close();
  await server.stop().catch(() => {});
});

const fake = () => {
  if (!server.workflows) throw new Error("this test needs the fake server");
  return server.workflows;
};

async function open(path: string, title: string | RegExp) {
  await page.goto(`${server.url}${path}`);
  await page.getByRole("heading", { name: title, level: 1 }).waitFor();
}

describe.skipIf(usesRealServer)("the project's Workflows tab", () => {
  it("loads, with what starts each workflow and how its last run went", async () => {
    await fresh();
    await open("/projects/1/workflows", "demo");
    const release = page.getByTestId("workflow-row").filter({ hasText: "release" });
    await release.getByRole("heading", { name: "release" }).waitFor();
    await release.getByText("Build, test and publish a release.").waitFor();
    await release.getByText("Schedule", { exact: true }).waitFor();
    await release.getByText("Failed", { exact: true }).waitFor();
    const deploy = page.getByTestId("workflow-row").filter({ hasText: "deploy" });
    await deploy.getByRole("heading", { name: "deploy" }).waitFor();
    await deploy.getByText("Manual", { exact: true }).waitFor();
  });

  it("opens a workflow's page from its name, on the Overview", async () => {
    await fresh();
    await open("/projects/1/workflows", "demo");
    await page.getByRole("link", { name: "release", exact: true }).click();
    await page.getByRole("heading", { name: "release", level: 1 }).waitFor();
    await page.getByRole("tab", { name: "Overview", selected: true }).waitFor();
    await page.getByRole("heading", { name: "Steps" }).waitFor();
  });
});

describe.skipIf(usesRealServer)("a workflow's page", () => {
  const tab = (name: string) => page.getByRole("tab", { name });

  it("has a header with the way back, what starts it, the four tabs and Run", async () => {
    await fresh();
    await open("/projects/1/workflows/release", "release");
    const trail = page.getByRole("navigation", { name: "Where this is" });
    await trail.getByRole("link", { name: "demo" }).waitFor();
    await trail.getByRole("link", { name: "Workflows" }).waitFor();
    await page
      .getByText(/^Schedule · next /)
      .first()
      .waitFor();
    for (const name of ["Overview", "Canvas", "Runs", "File"]) await tab(name).waitFor();
    await page.getByRole("button", { name: "Run", exact: true }).waitFor();
    // Not a webhook, so there is no hook to copy.
    expect(await page.getByRole("button", { name: "Copy hook URL" }).count()).toBe(0);
  });

  it("lists the steps of the Overview in words, without JSON or an escaped line break", async () => {
    await fresh();
    await open("/projects/1/workflows/notify", "notify");
    const steps = page.getByTestId("workflow-step");
    await steps.first().waitFor();
    expect(await steps.count()).toBe(3);
    const first = steps.nth(0);
    await first.getByText("Agent · coder").waitFor();
    await first.getByText("{{ trigger.ref }}").waitFor();
    const text = (await first.textContent()) ?? "";
    expect(text).toContain("A push arrived:");
    expect(text).toContain("…");
    expect(text).not.toContain("\\n");
    expect(text).not.toMatch(/\{"|":/);
    await steps.nth(1).getByText("Approval").waitFor();
    await steps.nth(2).getByText("Publish").waitFor();
    // A webhook says where it answers, and its header copies it.
    await page.getByText("/api/hooks/projects/1/notify").first().waitFor();
    expect((await page.locator("body").textContent()) ?? "").not.toContain("\\n");
  });

  it("copies the hook URL of a webhook from the header", async () => {
    await fresh();
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    await open("/projects/1/workflows/notify", "notify");
    await page.getByRole("button", { name: "Copy hook URL" }).click();
    await page.getByRole("button", { name: "Copied" }).waitFor();
    expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(
      "/api/hooks/projects/1/notify",
    );
  });

  it("moves between the tabs by their addresses", async () => {
    await fresh();
    await open("/projects/1/workflows/release", "release");
    await tab("Canvas").click();
    await page.waitForURL(/\/projects\/1\/workflows\/release\/canvas$/);
    await page.getByTestId("canvas-page").waitFor();
    await tab("Runs").click();
    await page.waitForURL(/\/projects\/1\/workflows\/release\/runs$/);
    await page.getByTestId("workflow-run-row").first().waitFor();
    await tab("File").click();
    await page.waitForURL(/\/projects\/1\/workflows\/release\/file$/);
    await page.getByText(/name = "release"/).waitFor();
    await page.getByRole("button", { name: /Copy code/ }).waitFor();
    await tab("Overview").click();
    await page.waitForURL(/\/projects\/1\/workflows\/release$/);
    await page.getByRole("heading", { name: "Steps" }).waitFor();
  });

  it("shows the file inside its box however long its lines are", async () => {
    await fresh();
    await open("/projects/1/workflows/notify/file", "notify");
    const box = page.getByTestId("workflow-file");
    await box.getByText(/name = "notify"/).waitFor();
    const fits = await box.evaluate((element) => {
      const frame = element.getBoundingClientRect();
      const pre = element.querySelector("pre");
      if (!pre) return false;
      const edge = pre.getBoundingClientRect();
      // Cut off on the right would be a pre wider than its frame with nothing to scroll it.
      return (
        edge.right <= frame.right + 1 &&
        (pre.scrollWidth <= pre.clientWidth || getComputedStyle(pre).overflowX !== "visible")
      );
    });
    expect(fits).toBe(true);
  });

  it("opens the Run dialog from the header", async () => {
    await fresh();
    await open("/projects/1/workflows/deploy", "deploy");
    await page.getByRole("button", { name: "Run", exact: true }).click();
    await page.getByRole("dialog", { name: "Run deploy" }).waitFor();
  });

  it("opens a run from the Runs tab", async () => {
    await fresh();
    await open("/projects/1/workflows/release/runs", "release");
    await page.getByRole("link", { name: "Run 41" }).click();
    await page.getByRole("heading", { name: "release · run 41", level: 1 }).waitFor();
  });

  it("sends the old addresses to the right tab", async () => {
    await fresh();
    await page.goto(`${server.url}/workflows/release/canvas?project=1`);
    await page.waitForURL(/\/projects\/1\/workflows\/release\/canvas$/);
    await page.getByTestId("canvas-page").waitFor();
    await page.goto(`${server.url}/workflows/release?project=1`);
    await page.waitForURL(/\/projects\/1\/workflows\/release$/);
    await page.getByRole("tab", { name: "Overview", selected: true }).waitFor();
  });
});

describe.skipIf(usesRealServer)("a run that failed at a node", () => {
  it("shows the status, each node's input, output and error, and the redaction marks", async () => {
    await fresh();
    await open("/workflow-runs/41", "release · run 41");
    await page.getByRole("region", { name: "Run" }).getByText("Failed", { exact: true }).waitFor();
    const test = page.getByRole("region", { name: "Node test" });
    await test.getByText(/\[redacted\]/).waitFor();
    await test.getByText(/expected `done`, got `in progress`/).waitFor();
    await page.getByText("The test node failed.", { exact: true }).waitFor();
  });

  it("shows all of a large value when asked", async () => {
    await fresh();
    await open("/workflow-runs/41", "release · run 41");
    const build = page.getByRole("region", { name: "Node build" });
    const output = build.locator('[data-json="build-output"]');
    await output.waitFor();
    expect(await output.getAttribute("data-complete")).toBe("false");
    await build.getByRole("button", { name: /Show all/ }).click();
    await expect.poll(() => output.getAttribute("data-complete")).toBe("true");
    await expect.poll(async () => (await output.textContent())?.includes('done"')).toBe(true);
  });

  it("moves with the node and run events without a reload", async () => {
    await fresh();
    await open("/workflow-runs/41", "release · run 41");
    const run = fake().runs.get(41);
    const test = run?.nodes.find((node) => node.node === "test");
    if (!run || !test) throw new Error("the fake failed run is not what the test expects");

    test.status = "succeeded";
    test.error = null;
    test.output = { passed: 62, failed: 0 };
    server.broadcast?.({
      kind: "workflow_node_ended",
      data: { run: 41, node: "test", attempt: 1, status: "succeeded", error: null },
    });
    const panel = page.getByRole("region", { name: "Node test" });
    await panel.getByText('"passed": 62').waitFor();
    await panel.getByText("Done", { exact: true }).waitFor();

    run.status = "succeeded";
    run.error = null;
    server.broadcast?.({
      kind: "workflow_run_ended",
      data: { run: 41, status: "succeeded", error: null },
    });
    await page.getByRole("region", { name: "Run" }).getByText("Done", { exact: true }).waitFor();
    await page.getByText("The test node failed.").waitFor({ state: "detached" });
  });
});

describe.skipIf(usesRealServer)("a run that waits", () => {
  it("says where it stopped, what it asks and which node is waiting", async () => {
    await fresh();
    await open("/workflow-runs/42", "deploy · run 42");
    await page.getByRole("region", { name: "Run" }).getByText("Waiting", { exact: true }).waitFor();
    const waiting = page.getByRole("region", { name: "Waiting" });
    await waiting.getByRole("heading", { name: "Waiting at approve" }).waitFor();
    await waiting.getByText(/Publish 1.4.0 to the registry\?/).waitFor();
    await waiting.getByText(/\[redacted\]/).waitFor();
    const approve = page.getByRole("region", { name: "Node approve" });
    await approve.waitFor();
    await expect.poll(() => approve.getAttribute("aria-current")).toBe("step");
  });
});

/** A server with no project has nowhere to keep a workflow (ADR 0025): makes one when there is none; its id. */
async function ensureProject(): Promise<{ id: number; name: string }> {
  const headers = { "X-Moebius": "1", Authorization: `Bearer ${server.token}` };
  const listed = await context.request.get(`${server.url}/v1/projects`, { headers });
  const first = ((await listed.json()) as Array<{ id: number; name: string }>)[0];
  if (first) return first;
  const made = await context.request.post(`${server.url}/v1/projects`, {
    headers,
    data: { name: "E2E", key: "E2E" },
  });
  expect(made.ok(), await made.text()).toBe(true);
  return (await made.json()) as { id: number; name: string };
}

/** Saves a workflow the way the command line does, so a real server and the fake both hold it. */
async function saveWorkflow(name: string): Promise<{ id: number; name: string }> {
  const project = await ensureProject();
  const response = await context.request.put(
    `${server.url}/v1/projects/${project.id}/workflows/${name}`,
    {
      headers: { "X-Moebius": "1", Authorization: `Bearer ${server.token}` },
      data: {
        definition: {
          name,
          description: "Made by the end-to-end test.",
          trigger: [{ kind: "manual", params: {} }],
          nodes: [
            {
              id: "shape",
              kind: "code",
              params: { source: "return input" },
              position: { x: 320, y: 120 },
            },
          ],
          edges: [{ from: "trigger", to: "shape", port: "out" }],
        },
      },
    },
  );
  if (!response.ok())
    throw new Error(`saving ${name}: ${response.status()} ${await response.text()}`);
  return project;
}

describe("a workflow saved through the API (a real server and the fake alike)", () => {
  it("is listed with its trigger and as never run, and opens with its nodes", async () => {
    await fresh();
    const project = await saveWorkflow("wf1");
    const errors: string[] = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await open(`/projects/${project.id}/workflows`, project.name);
    const row = page.getByTestId("workflow-row").filter({ hasText: "wf1" });
    await row.getByText("Manual", { exact: true }).waitFor();
    await row.getByText("Never run").waitFor();

    await row.getByRole("link", { name: "wf1" }).click();
    await page.getByRole("heading", { name: "wf1", level: 1 }).waitFor();
    const graph = page.getByRole("region", { name: "Definition graph" });
    await graph.getByRole("heading", { name: "shape" }).waitFor();
    await graph.getByText("code", { exact: true }).waitFor();
    await page.getByText("This workflow has not run.").first().waitFor();
    await page.getByRole("button", { name: "Run" }).waitFor();
    expect(errors).toEqual([]);
  });

  it("opens the canvas of a name that is not saved yet blank, after a reload, without a failed read", async () => {
    await fresh();
    const failed: string[] = [];
    page.on("response", (response) => {
      if (response.status() >= 400) failed.push(`${response.status()} ${response.url()}`);
    });
    const project = await ensureProject();
    await page.goto(`${server.url}/projects/${project.id}/workflows/notyet/canvas`);
    await page
      .getByRole("heading", { name: /notyet/ })
      .first()
      .waitFor();
    await page.getByRole("button", { name: /Save/ }).waitFor();
    await page
      .getByText(/could not be read|cannot be read|os error/i)
      .waitFor({ state: "detached" });
    await page.reload();
    await page.getByRole("button", { name: /Save/ }).waitFor();
    // The one read of the workflow answers 404 and is not an error of the page; the runs are not asked for.
    expect(failed.filter((entry) => entry.includes("workflow-runs"))).toEqual([]);
  });
});
