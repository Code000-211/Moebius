/**
 * The Workflows pages in a real browser: the list with each workflow's trigger and last run, a definition's saved graph
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

describe.skipIf(usesRealServer)("the Workflows list", () => {
  it("loads, with what starts each workflow and how its last run went", async () => {
    await fresh();
    await open("/workflows", "Workflows");
    await page.getByRole("heading", { name: "release" }).waitFor();
    await page.getByText("Build, test and publish a release.").waitFor();
    await page.getByText("Schedule", { exact: true }).first().waitFor();
    await page.getByText("3 nodes").waitFor();
    await page.getByText(/Last run: failed/).waitFor();
    await page.getByRole("heading", { name: "deploy" }).waitFor();
    await page.getByText("Manual", { exact: true }).waitFor();
  });

  it("opens a workflow's definition from its name", async () => {
    await fresh();
    await open("/workflows", "Workflows");
    await page.getByRole("link", { name: "release" }).click();
    await page.getByRole("heading", { name: "release", level: 1 }).waitFor();
    await page.getByRole("region", { name: "Definition graph" }).waitFor();
  });
});

describe.skipIf(usesRealServer)("a workflow's definition", () => {
  it("draws the saved graph with each node's kind and parameters, and shows the file", async () => {
    await fresh();
    await open("/workflows/release", "release");
    const graph = page.getByRole("region", { name: "Definition graph" });
    await graph.getByRole("heading", { name: "build" }).waitFor();
    await graph.getByText("agent", { exact: true }).waitFor();
    await graph.getByText(/Build the release/).waitFor();
    await graph.getByRole("heading", { name: "test" }).waitFor();
    await graph.getByText(/cargo test --workspace/).waitFor();
    await page.getByRole("heading", { name: "File" }).waitFor();
    await page.getByText(/name = "release"/).waitFor();
    await page.getByRole("button", { name: /Copy code/ }).waitFor();
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

/** Saves a workflow the way the command line does, so a real server and the fake both hold it. */
async function saveWorkflow(name: string): Promise<void> {
  const response = await context.request.put(`${server.url}/v1/workflows/${name}`, {
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
  });
  if (!response.ok())
    throw new Error(`saving ${name}: ${response.status()} ${await response.text()}`);
}

describe("a workflow saved through the API (a real server and the fake alike)", () => {
  it("is listed with its trigger and as never run, and opens with its nodes", async () => {
    await fresh();
    await saveWorkflow("wf1");
    const errors: string[] = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await open("/workflows", "Workflows");
    const row = page.getByTestId("workflow-row").filter({ hasText: "wf1" });
    await row.getByText("Manual", { exact: true }).waitFor();
    await row.getByText("1 node", { exact: true }).waitFor();
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
    await page.goto(`${server.url}/workflows/notyet/canvas`);
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
