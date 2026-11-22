/**
 * A project's Workflows tab in a real browser, against the fake server (e2e/fake/workflows.ts): its definitions and
 * runs, the run filter kept in the address, "Run a workflow here", the command bar's way to the tab, the old addresses
 * that now lead to it, and deleting a workflow from its definition page. Delete is asked in a dialog, which warns that
 * a workflow shaped like an automation stops one; a refusal of a run still queued or running is said in the dialog and
 * the page stays; a success returns to the tab. Every wait is on something the page shows, never on time.
 */
import { type Browser, type BrowserContext, chromium, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { pickOption } from "./controls";
import type { FakeServer } from "./fakeServer";
import { startServer, type TestServer, usesRealServer } from "./server";

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

afterEach(async () => {
  await context.close();
  await server.stop().catch(() => {});
});

async function open(path: string) {
  server = await startServer();
  server.pagesB?.use();
  context = await browser.newContext({ viewport: { width: 1280, height: 900 } });
  page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  await page.goto(`${server.url}${path}`);
}

function fake(): FakeServer["workflows"] {
  if (!server.workflows) throw new Error("these tests run against the fake server");
  return server.workflows;
}

const requests = (method: string, path: string) =>
  fake().calls.filter((call) => call.method === method && call.path === path);

const TAB = "/projects/1/workflows";
const definition = (page: Page, name: string) =>
  page.getByTestId("workflow-row").filter({ has: page.getByRole("link", { name, exact: true }) });

describe.skipIf(usesRealServer)("a project's Workflows tab", () => {
  it("lists the project's workflows and its runs", async () => {
    await open(TAB);
    await definition(page, "release").waitFor();
    await definition(page, "deploy").waitFor();
    await definition(page, "tidy").waitFor();
    await definition(page, "release").getByText("Failed", { exact: true }).waitFor();
    await definition(page, "deploy").getByText("Manual", { exact: true }).waitFor();
    const runs = page.getByTestId("run-row");
    await runs.first().waitFor();
    expect(await runs.count()).toBe(2);
    await runs
      .filter({ hasText: "release" })
      .getByText(/demo · failed/)
      .waitFor();
    await runs
      .filter({ hasText: "deploy" })
      .getByText(/demo · waiting/)
      .waitFor();
    expect(requests("GET", "/v1/projects/1/workflows")).not.toHaveLength(0);
  });

  it("filters the runs All, Scheduled or Failed, and keeps the filter in the address across a reload", async () => {
    await open(TAB);
    const runs = page.getByTestId("run-row");
    await runs.first().waitFor();
    expect(await runs.count()).toBe(2);

    await page.getByRole("tab", { name: "Failed" }).click();
    await page.waitForURL(/\?runs=failed$/);
    await expect.poll(() => runs.count()).toBe(1);
    await runs.filter({ hasText: "release" }).waitFor();

    await page.reload();
    await page.getByRole("tab", { name: "Failed", selected: true }).waitFor();
    await runs.filter({ hasText: "release" }).waitFor();
    expect(await runs.count()).toBe(1);

    await page.getByRole("tab", { name: "Scheduled" }).click();
    await page.waitForURL(/\?runs=scheduled$/);
    await page.getByTestId("scheduled-row").filter({ hasText: "tidy" }).waitFor();
    await expect.poll(() => runs.count()).toBe(1);

    await page.getByRole("tab", { name: "All" }).click();
    await expect.poll(() => runs.count()).toBe(2);
    expect(new URL(page.url()).search).toBe("");
  });

  it("starts a run with 'Run a workflow here' in one of the project's repositories and opens it", async () => {
    await open(TAB);
    await page.getByRole("button", { name: "Run a workflow here" }).click();
    const dialog = page.getByRole("dialog", { name: "Run a workflow in demo" });
    await dialog.waitFor();
    // The workflows started by hand are offered, and the repository is chosen among the project's.
    const repository = dialog.getByRole("button", { name: "Repository" });
    await pickOption(page, repository, "demo");
    await dialog.getByRole("button", { name: "Run", exact: true }).click();
    await page.waitForURL(/\/workflow-runs\/43$/);
    expect(requests("POST", "/v1/projects/1/workflows/deploy/runs")).toHaveLength(1);
    await page.getByRole("heading", { name: "deploy · run 43", level: 1 }).waitFor();
    await page.getByRole("link", { name: "demo workflows" }).click();
    await page.waitForURL(/\/projects\/1\/workflows$/);
    await definition(page, "deploy").waitFor();
  });

  it("is opened by the command bar", async () => {
    await open("/inbox");
    await page.getByRole("navigation", { name: "Pages" }).waitFor();
    await page.keyboard.press("ControlOrMeta+k");
    const input = page.getByPlaceholder("Search…");
    await input.waitFor();
    await input.fill("Workflows");
    await page.getByRole("option", { name: "Workflows" }).first().waitFor();
    await page.keyboard.press("Enter");
    await page.waitForURL(/\/projects\/1\/workflows$/);
    await definition(page, "release").waitFor();
  });

  it("is where the old addresses lead", async () => {
    await open("/workflows");
    await page.waitForURL(/\/projects\/1\/workflows$/);
    await definition(page, "release").waitFor();

    await page.goto(`${server.url}/automations`);
    await page.waitForURL(/\/projects\/1\/workflows\?runs=scheduled$/);
    await page.getByRole("button", { name: "New scheduled workflow" }).waitFor();

    await page.goto(`${server.url}/workflows/release`);
    await page.waitForURL(/\/projects\/1\/workflows\/release$/);
    await page.getByRole("heading", { name: "release", level: 1 }).waitFor();
  });

  it("answers the routes that name no project as gone", async () => {
    await open(TAB);
    const asked = await page.evaluate(async () => {
      const response = await fetch("/v1/workflows");
      return response.status;
    });
    expect(asked).toBe(410);
  });
});

describe.skipIf(usesRealServer)("deleting a workflow from its detail page", () => {
  it("deletes a workflow and returns to the tab, which no longer names it", async () => {
    await open(TAB);
    await definition(page, "release").getByRole("link", { name: "release", exact: true }).click();
    await page.getByRole("tab", { name: "Overview" }).waitFor();
    await page.getByRole("button", { name: "More workflow actions" }).click();
    await page.getByRole("menuitem", { name: "Delete workflow" }).click();
    const dialog = page.getByRole("dialog", { name: "Delete release?" });
    await dialog.waitFor();
    await dialog.getByRole("button", { name: "Delete" }).click();
    await page.waitForURL(/\/projects\/1\/workflows$/);
    await definition(page, "deploy").waitFor();
    // The list is the query cache's stale copy until its refetch returns: poll for it to drop the deleted one.
    await expect.poll(async () => definition(page, "release").count()).toBe(0);
    expect(requests("DELETE", "/v1/projects/1/workflows/release")).toHaveLength(1);
  });

  it("warns that a workflow shaped like an automation stops one", async () => {
    await open(TAB);
    await definition(page, "tidy").getByRole("link", { name: "tidy", exact: true }).click();
    await page.getByRole("tab", { name: "Overview" }).waitFor();
    await page.getByRole("button", { name: "More workflow actions" }).click();
    await page.getByRole("menuitem", { name: "Delete workflow" }).click();
    const dialog = page.getByRole("dialog", { name: "Delete tidy?" });
    await dialog.waitFor();
    await dialog.getByText(/automation: a schedule that starts one chat/).waitFor();
  });

  it("says the refusal of a run still queued or running, and stays on the page", async () => {
    await open(TAB);
    await definition(page, "release").getByRole("link", { name: "release", exact: true }).click();
    await page.getByRole("tab", { name: "Overview" }).waitFor();
    await page.getByRole("button", { name: "More workflow actions" }).click();
    await page.getByRole("menuitem", { name: "Delete workflow" }).click();
    const dialog = page.getByRole("dialog", { name: "Delete release?" });
    await dialog.waitFor();
    fake().refuseNextDelete(409, "workflow release has runs queued, running or waiting");
    await dialog.getByRole("button", { name: "Delete" }).click();
    await dialog.getByText(/has runs queued, running or waiting/).waitFor();
    await page.waitForURL(/\/projects\/1\/workflows\/release$/);
  });
});
