/**
 * Settings as one page in a real browser: the sub-navigation reaches Models, Plugins and Kinds of issues without leaving the
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
  it("reaches Models, Plugins and Kinds of issues from one layout and keeps the address on reload", async () => {
    server = await startServer();
    const page = await (await browser.newContext()).newPage();
    page.setDefaultTimeout(8000);
    await page.goto(server.loginUrl);
    await page.goto(`${server.url}/settings`);
    await nav(page).waitFor();
    for (const [label, path] of [
      ["Models", "models"],
      ["Plugins", "plugins"],
      ["Kinds of issues", "kinds"],
    ] as const) {
      await nav(page).getByRole("link", { name: label, exact: true }).click();
      await page.waitForURL(`**/${path}`);
      // An area titles itself as a heading of its own (a level 1 until it is flattened, a level 2 under "Settings" after).
      await page.getByRole("heading", { name: label, exact: true }).first().waitFor();
      await nav(page).waitFor();
    }
    await page.reload();
    await nav(page).waitFor();
    expect(page.url()).toMatch(/\/kinds$/);
    expect(
      await nav(page).getByRole("link", { name: "Kinds of issues" }).getAttribute("aria-current"),
    ).toBe("page");
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
    await expect.poll(() => nav(page).getByRole("link").count()).toBe(1);
    await search.fill("zzz");
    await nav(page).getByText("No settings match.").waitFor();
  });

  it("opens Profile, My tokens and Members from the navigation and by address, with no Ask field", async () => {
    server = await startServer();
    const page = await (await browser.newContext()).newPage();
    page.setDefaultTimeout(8000);
    await page.goto(server.loginUrl);
    await page.goto(`${server.url}/settings`);
    await nav(page).waitFor();
    for (const [label, path] of [
      ["Profile", "profile"],
      ["My tokens", "tokens"],
      ["Members", "people"],
    ] as const) {
      await nav(page).getByRole("link", { name: label, exact: true }).click();
      await page.waitForURL(`**/${path}`);
      await page.getByRole("heading", { name: label, exact: true }).first().waitFor();
      await expect.poll(() => page.getByLabel("Ask Moebius").count()).toBe(0);
      await page.reload();
      await page.getByRole("heading", { name: label, exact: true }).first().waitFor();
    }
  });

  it("lists exactly the nine entries in the two groups, and Server > General shows the nightly times", async () => {
    server = await startServer();
    const page = await (await browser.newContext()).newPage();
    page.setDefaultTimeout(8000);
    await page.goto(server.loginUrl);
    await page.goto(`${server.url}/settings`);
    await nav(page).waitFor();
    const groups = ["You", "Server"];
    const shown = await nav(page).evaluate((element, names) => {
      const out: Array<[string, string]> = [];
      let group = "";
      for (const node of element.querySelectorAll("a, *")) {
        const text = node.children.length === 0 ? (node.textContent ?? "").trim() : "";
        if (node.tagName !== "A" && names.includes(text)) group = text;
        if (node.tagName === "A") out.push([group, (node.textContent ?? "").trim()]);
      }
      return out;
    }, groups);
    expect(shown).toEqual([
      ["You", "Profile"],
      ["You", "Appearance"],
      ["You", "Keyboard shortcuts"],
      ["You", "My tokens"],
      ["Server", "General"],
      ["Server", "Models"],
      ["Server", "Plugins"],
      ["Server", "Kinds of issues"],
      ["Server", "Members"],
    ]);
    await page.getByRole("heading", { name: "Nightly", exact: true }).waitFor();
    await page.getByText("Retro at", { exact: false }).first().waitFor();
  });

  it("keeps Show thinking expanded on the server, so it is still on after a reload", async () => {
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
