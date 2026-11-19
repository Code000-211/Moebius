/**
 * The Views page in a real browser: it lists the views, opens one to its cards drawn in its layout (board lanes, a
 * calendar by due date, a timeline by cycle), edits and shares one, and deletes one. They run against the fake server
 * of `fake/views.ts`; which cards a person may see is the server's own rule and tested there. Every wait is on something
 * the page shows, never on time.
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

async function open(path: string) {
  server = await startServer();
  context = await browser.newContext({ viewport: { width: 1400, height: 1000 } });
  page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  await page.goto(`${server.url}${path}`);
}

const fake = () => {
  if (!("views" in server)) throw new Error("this test needs the fake server");
  return server.views;
};

describe.skipIf(usesRealServer)("the Views page", () => {
  it("is in the sidebar and opens a shared view to its cards in lanes", async () => {
    await open("/inbox");
    await page
      .getByRole("navigation", { name: "Pages" })
      .getByRole("link", { name: "Views" })
      .click();
    await page.getByRole("heading", { name: "Views", level: 1 }).waitFor();
    await page.getByRole("link", { name: /Team board/ }).click();
    await page.getByTestId("view").waitFor();
    await page.getByText("Shows column In progress").waitFor();
    await page.getByTestId("view-card-1").waitFor();
    expect(await page.getByTestId("view-card-2").count()).toBe(0);
    expect(await page.getByTestId("view-group").count()).toBe(1);
  });

  it("edits a view into a timeline, and the cards are set against their cycle", async () => {
    await open("/views/1");
    await page.getByTestId("view").waitFor();
    await page.getByRole("button", { name: "Edit" }).click();
    const dialog = page.getByRole("dialog", { name: "Edit view" });
    await dialog.getByRole("button", { name: "Layout" }).click();
    await page.getByRole("menuitemradio", { name: "Timeline" }).click();
    await dialog.getByLabel("Share with every member").uncheck();
    await dialog.getByRole("button", { name: "Save" }).click();
    await dialog.waitFor({ state: "detached" });
    await page.getByText("Cycle 1 · 2026-11-02 to 2026-11-15").waitFor();
    await page.getByText("No cycle").waitFor();
    await page.getByText(/Only you/).waitFor();
    expect(fake().requests.at(-1)).toMatchObject({
      method: "PATCH",
      path: "/v1/views/1",
      body: { layout: "timeline", shared: false },
    });
  });

  it("deletes a view", async () => {
    await open("/views/1");
    await page.getByTestId("view").waitFor();
    await page.getByRole("button", { name: "Delete" }).click();
    await page.getByRole("dialog").getByRole("button", { name: "Delete" }).click();
    await page.getByRole("link", { name: /Team board/ }).waitFor({ state: "detached" });
    expect(fake().requests.at(-1)).toMatchObject({ method: "DELETE", path: "/v1/views/1" });
  });

  it("offers Edit and Delete only for a view the person may change", async () => {
    await open("/views/2");
    await page.getByTestId("view").waitFor();
    expect(await page.getByRole("button", { name: "Edit" }).count()).toBe(0);
    expect(await page.getByRole("button", { name: "Delete" }).count()).toBe(0);
    await page.getByRole("link", { name: /Team board/ }).click();
    await page.getByRole("button", { name: "Edit" }).waitFor();
  });
});
