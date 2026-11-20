/**
 * Settings as one page in a real browser: the sub-navigation reaches Models, Rules and Retro without leaving the
 * layout, keeps the address on reload, and narrows by what is typed in its search.
 */
import { type Browser, chromium, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { startServer, type TestServer } from "./server";

let browser: Browser;
let server: TestServer;

beforeAll(async () => {
  browser = await chromium.launch({
    executablePath: process.env.MOEBIUS_E2E_CHROMIUM || undefined,
  });
});
afterAll(async () => {
  await browser.close();
});
afterEach(async () => {
  await server.stop().catch(() => {});
});

const nav = (page: Page) => page.getByRole("navigation", { name: "Settings" });

describe("Settings", () => {
  it("reaches Models, Rules and Retro from one layout and keeps the address on reload", async () => {
    server = await startServer();
    const page = await (await browser.newContext()).newPage();
    page.setDefaultTimeout(8000);
    await page.goto(server.loginUrl);
    await page.goto(`${server.url}/settings`);
    await nav(page).waitFor();
    for (const [label, path] of [
      ["Models", "models"],
      ["Rules", "rules"],
      ["Retro", "retro"],
    ] as const) {
      await nav(page).getByRole("link", { name: label, exact: true }).click();
      await page.waitForURL(`**/${path}`);
      await page.getByRole("heading", { level: 1, name: label }).waitFor();
      await nav(page).waitFor();
    }
    await page.reload();
    await nav(page).waitFor();
    expect(page.url()).toMatch(/\/retro$/);
    expect(await nav(page).getByRole("link", { name: "Retro" }).getAttribute("aria-current")).toBe(
      "page",
    );
  });

  it("narrows the entries by search, and says when nothing matches", async () => {
    server = await startServer();
    const page = await (await browser.newContext()).newPage();
    page.setDefaultTimeout(8000);
    await page.goto(server.loginUrl);
    await page.goto(`${server.url}/settings`);
    const search = nav(page).getByRole("searchbox", { name: "Search settings" });
    await search.fill("mod");
    await nav(page).getByRole("link", { name: "Models" }).waitFor();
    expect(await nav(page).getByRole("link").count()).toBe(1);
    await search.fill("zzz");
    await nav(page).getByText("No settings match.").waitFor();
  });

  it("keeps Show thinking expanded on the server, so it is still on after a reload and in a new browser", async () => {
    server = await startServer();
    const page = await (await browser.newContext()).newPage();
    page.setDefaultTimeout(8000);
    await page.goto(server.loginUrl);
    await page.goto(`${server.url}/appearance`);
    const row = page.getByRole("switch", { name: "Show thinking expanded" });
    expect(await row.getAttribute("aria-checked")).toBe("false");
    const saved = page.waitForResponse(
      (response) =>
        response.url().endsWith("/v1/preferences") && response.request().method() === "PUT",
    );
    await row.click();
    await saved;
    await page.reload();
    await page
      .getByRole("switch", { name: "Show thinking expanded" })
      .and(page.locator("[aria-checked='true']"))
      .waitFor();
    // Nothing is kept in this browser: the value is the person's record on the server.
    expect(await page.evaluate(() => JSON.stringify({ ...localStorage }))).not.toMatch(/thinking/i);
  });
});
