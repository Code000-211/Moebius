/**
 * The workflow pages in a real browser, against the fake server (e2e/fake/workflows.ts): the list, the definition page
 * and deleting a workflow from its detail page. Delete is asked in a dialog, which warns that a workflow shaped like an
 * automation stops one; a refusal of a run still queued or running is said in the dialog and the page stays; a success
 * returns to the list. Every wait is on something the page shows, never on time.
 */
import { type Browser, type BrowserContext, chromium, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
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

describe.skipIf(usesRealServer)("deleting a workflow from its detail page", () => {
  it("deletes a workflow and returns to the list, which no longer names it", async () => {
    await open("/workflows");
    await page.getByRole("link", { name: "release" }).click();
    await page.getByRole("button", { name: "Delete workflow" }).click();
    const dialog = page.getByRole("dialog", { name: "Delete release?" });
    await dialog.waitFor();
    await dialog.getByRole("button", { name: "Delete" }).click();
    await page.waitForURL(/\/workflows$/);
    await page.getByRole("link", { name: "deploy" }).waitFor();
    // The list is the query cache's stale copy until its refetch returns: poll for it to drop the deleted one.
    await expect.poll(async () => page.getByRole("link", { name: "release" }).count()).toBe(0);
    expect(requests("DELETE", "/v1/workflows/release")).toHaveLength(1);
  });

  it("warns that a workflow shaped like an automation stops one", async () => {
    await open("/workflows");
    await page.getByRole("link", { name: "tidy" }).click();
    await page.getByRole("button", { name: "Delete workflow" }).click();
    const dialog = page.getByRole("dialog", { name: "Delete tidy?" });
    await dialog.waitFor();
    await dialog.getByText(/automation: a schedule that starts one chat/).waitFor();
  });

  it("says the refusal of a run still queued or running, and stays on the page", async () => {
    await open("/workflows");
    await page.getByRole("link", { name: "release" }).click();
    await page.getByRole("button", { name: "Delete workflow" }).click();
    const dialog = page.getByRole("dialog", { name: "Delete release?" });
    await dialog.waitFor();
    fake().refuseNextDelete(409, "workflow release has runs queued, running or waiting");
    await dialog.getByRole("button", { name: "Delete" }).click();
    await dialog.getByText(/has runs queued, running or waiting/).waitFor();
    await page.waitForURL(/\/workflows\/release$/);
  });
});
