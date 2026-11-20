/**
 * The Fleet list with many runs, in a real browser against the fake server: 500 running chats are in the list, only the
 * rows in view are drawn, the page follows no more runs than it draws, the run that waits for the person is the first
 * row, and Stop is in the page only on the row that is hovered or selected.
 */
import { type Browser, type BrowserContext, chromium, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
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

async function fresh() {
  server = await startServer();
  context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
}

describe.skipIf(usesRealServer)("the fleet with 500 runs", () => {
  it("draws and follows only the rows in view, with Stop on the hovered row alone", async () => {
    await fresh();
    for (let n = 1; n <= 500; n++) server.chat?.seed({ status: "running", title: `Run ${n}` });
    server.chat?.seed({ status: "waiting", asks: true, title: "Which branch?" });
    await page.goto(`${server.url}/fleet`);
    const rows = page.getByTestId("list-row");
    await rows.first().waitFor();
    await expect.poll(() => server.watchedRuns?.() ?? 0).toBeGreaterThan(0);
    const drawn = await rows.count();
    expect(drawn).toBeLessThan(80);
    expect(server.openStreams?.() ?? 0).toBeGreaterThan(0);
    expect(server.openStreams?.() ?? 0).toBeLessThanOrEqual(drawn);
    expect(server.watchedRuns?.() ?? 0).toBeLessThanOrEqual(drawn);

    // The run that waits for the person is first, under its own group.
    expect(await page.getByTestId("group-row").first().textContent()).toBe("Needs you");
    expect(await page.getByTestId("group-row").first().getAttribute("aria-posinset")).toBe("1");
    await rows.first().getByText("Which branch?").waitFor();
    await rows.first().getByText("Asks you").waitFor();

    // Stop is not in the page until a row is hovered or selected.
    expect(await page.getByRole("button", { name: /^Stop/ }).count()).toBe(0);
    await rows.nth(1).hover();
    expect(await page.getByRole("button", { name: /^Stop/ }).count()).toBe(1);
    await page.mouse.move(700, 5);
    await expect.poll(() => page.getByRole("button", { name: /^Stop/ }).count()).toBe(0);
  });
});

describe.skipIf(usesRealServer)("the Finished tab", () => {
  it("keeps the tab in the address through a reload and lists only runs that ended", async () => {
    await fresh();
    await page.goto(`${server.url}/fleet`);
    await page.getByRole("tab", { name: "Finished" }).click();
    await page.waitForURL(/\/fleet\?tab=finished$/);
    const rows = page.getByTestId("list-row");
    await rows.first().waitFor();
    // Three runs of the fake have ended (a failed, a succeeded, a stopped one); the two going on are not among them.
    await expect.poll(() => rows.count()).toBe(3);
    expect(await page.getByTestId("outcome-failed").count()).toBe(1);
    expect(await page.getByTestId("outcome-succeeded").count()).toBe(1);
    expect(await page.getByTestId("outcome-stopped").count()).toBe(1);
    await page.getByTestId("group-row").first().getByText("Today").waitFor();

    await page.reload();
    await rows.first().waitFor();
    expect(page.url()).toMatch(/\/fleet\?tab=finished$/);
    expect(await page.getByRole("tab", { name: "Finished" }).getAttribute("aria-selected")).toBe(
      "true",
    );
    await expect.poll(() => rows.count()).toBe(3);

    await page.getByRole("tab", { name: "Active" }).click();
    await page.waitForURL(/\/fleet$/);
  });
});
