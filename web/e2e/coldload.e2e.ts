/**
 * What a cold load asks of the server: no URL twice on a quiet server (the event stream's first connect does not read
 * again what was just read), a card's requests that do not wait for the snapshot, and the chat and its Markdown that
 * are fetched when the Moebius agent is first opened, not with the page. The fake server (`fake/board.ts`) has the data, so
 * the tests are skipped against a real server. Every wait is on something the page shows, never on time.
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

/** A signed-in browser that has not opened a page of the app yet, and the API reads and files it will make. */
async function fresh() {
  server = await startServer();
  context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  // A new page, as when the app is opened in a new tab: its caches are empty.
  await page.close();
  page = await context.newPage();
}

function record(): { api: string[]; files: string[] } {
  const seen = { api: [] as string[], files: [] as string[] };
  page.on("request", (request) => {
    const url = new URL(request.url());
    if (request.method() === "GET" && url.pathname.startsWith("/v1/"))
      seen.api.push(url.pathname + url.search);
    else if (url.pathname.startsWith("/assets/")) seen.files.push(url.pathname);
  });
  return seen;
}

describe.skipIf(usesRealServer)("a cold load", () => {
  it("asks for no URL twice on a quiet server", async () => {
    await fresh();
    const seen = record();
    await page.goto(`${server.url}/board/1`);
    await page.getByRole("heading", { name: "demo", level: 1 }).waitFor();
    // The event stream is up, which is the moment it used to read everything again.
    await page.waitForLoadState("networkidle");
    const twice = seen.api.filter((path, i) => seen.api.indexOf(path) !== i);
    expect(twice).toEqual([]);
  });

  it("starts a card's requests without waiting for the snapshot", async () => {
    await fresh();
    let release = () => {};
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    await page.route("**/v1/snapshot", async (route) => {
      await held;
      await route.continue();
    });
    const seen = record();
    try {
      await page.goto(`${server.url}/card/DEMO-31`);
      // The card's page is read while the snapshot is held.
      await expect
        .poll(() => seen.api.some((path) => /^\/v1\/cards\/\d+\/comments$/.test(path)))
        .toBe(true);
      expect(seen.api.some((path) => /^\/v1\/cards\/by-key\/DEMO-31$/.test(path))).toBe(true);
    } finally {
      release();
    }
    // The card stays on its own page, with the way back to its board, once the snapshot arrives.
    await page.getByRole("link", { name: "Back to the board" }).waitFor();
    expect(new URL(page.url()).pathname).toBe("/card/DEMO-31");
    await page.waitForLoadState("networkidle");
    const comments = seen.api.filter((path) => /^\/v1\/cards\/\d+\/comments$/.test(path));
    expect(comments).toHaveLength(1);
  });

  it("loads the chat when the Moebius agent is first opened", async () => {
    await fresh();
    const seen = record();
    await page.goto(`${server.url}/board/1`);
    await page.getByRole("heading", { name: "demo", level: 1 }).waitFor();
    await page.waitForLoadState("networkidle");
    expect(seen.files.filter((file) => /\/AgentChat-/.test(file))).toEqual([]);
    await page.keyboard.press("Control+j");
    await page.getByRole("complementary", { name: "Moebius agent" }).waitFor();
    expect(seen.files.some((file) => /\/AgentChat-/.test(file))).toBe(true);
  });
});
