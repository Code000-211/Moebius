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

  it("opens Profile, Notifications and Repositories from the navigation and by address, with no Ask field", async () => {
    server = await startServer();
    const page = await (await browser.newContext()).newPage();
    page.setDefaultTimeout(8000);
    await page.goto(server.loginUrl);
    await page.goto(`${server.url}/settings`);
    await nav(page).waitFor();
    for (const [label, path] of [
      ["Profile", "profile"],
      ["Notifications", "notifications"],
      ["Repositories", "repositories"],
    ] as const) {
      await nav(page).getByRole("link", { name: label, exact: true }).click();
      await page.waitForURL(`**/${path}`);
      await page.getByRole("heading", { level: 2, name: label }).waitFor();
      expect(await page.getByLabel("Ask Moebius").count()).toBe(0);
      await page.reload();
      await page.getByRole("heading", { level: 2, name: label }).waitFor();
    }
    await page.getByRole("link", { name: /Add a repository/ }).waitFor();
  });

  it("lists the twenty decided entries in their groups, and opens the four added ones with content", async () => {
    server = await startServer();
    const page = await (await browser.newContext()).newPage();
    page.setDefaultTimeout(8000);
    await page.goto(server.loginUrl);
    await page.goto(`${server.url}/settings`);
    await nav(page).waitFor();
    const groups = ["You", "Agents", "Automation", "Insight", "Server"];
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
      ["You", "Notifications"],
      ["You", "Keyboard"],
      ["Agents", "Agents"],
      ["Agents", "Models"],
      ["Agents", "Skills"],
      ["Agents", "Plugins"],
      ["Automation", "Rules"],
      ["Automation", "Workflows"],
      ["Automation", "Column scripts"],
      ["Automation", "Schedules"],
      ["Insight", "Analytics"],
      ["Insight", "Retro"],
      ["Insight", "Lessons"],
      ["Insight", "Export"],
      ["Server", "General"],
      ["Server", "Repositories"],
      ["Server", "Runners"],
      ["Server", "Members"],
    ]);
    for (const [label, path, content] of [
      ["Column scripts", "column-scripts", "Open"],
      ["Schedules", "schedules", "Retro"],
      ["Runners", "runners", "build.example.com"],
      ["Export", "export", "Open Decisions"],
    ] as const) {
      await nav(page).getByRole("link", { name: label, exact: true }).click();
      await page.waitForURL(`**/${path}`);
      await page.getByRole("heading", { level: 2, name: label }).waitFor();
      await page.getByText(content, { exact: false }).first().waitFor();
      expect(await page.getByLabel("Ask Moebius").count()).toBe(0);
    }
  });
});
