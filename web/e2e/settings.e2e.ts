/**
 * Settings as one page in a real browser: the sub-navigation reaches Models, Rules and Retro without leaving the
 * layout, keeps the address on reload, and narrows by what is typed in its search.
 */
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type Browser, chromium, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { startServer, type TestServer, usesRealServer } from "./server";

let browser: Browser;
let server: TestServer;
const directories: string[] = [];

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
  for (const directory of directories.splice(0))
    rmSync(directory, { recursive: true, force: true });
});

/** Gives a real server a project with a board, as the fake starts with one: Column scripts lists the board's columns. */
async function withProject(page: Page) {
  if (!usesRealServer) return;
  const directory = mkdtempSync(join(tmpdir(), "moebius-e2e-repo-"));
  directories.push(directory);
  const response = await page.request.post(`${server.url}/v1/repositories`, {
    headers: { "X-Moebius": "1", Authorization: `Bearer ${server.token}` },
    data: { name: "demo", url: directory },
  });
  expect(response.ok(), await response.text()).toBe(true);
}

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
      // An area titles itself as a heading of its own (a level 1 until it is flattened, a level 2 under "Settings" after).
      await page.getByRole("heading", { name: label, exact: true }).first().waitFor();
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
    await expect.poll(() => nav(page).getByRole("link").count()).toBe(1);
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
      await expect.poll(() => page.getByLabel("Ask Moebius").count()).toBe(0);
      await page.reload();
      await page.getByRole("heading", { level: 2, name: label }).waitFor();
    }
    await page.getByRole("link", { name: /Add a repository/ }).waitFor();
  });

  it("lists the decided entries in their groups, and opens the four added ones with content", async () => {
    server = await startServer();
    const page = await (await browser.newContext()).newPage();
    page.setDefaultTimeout(8000);
    await page.goto(server.loginUrl);
    await withProject(page);
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
      ["Agents", "MCP servers"],
      ["Automation", "Rules"],
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
      // The fake has a runner; a new real server has none and says so.
      ["Runners", "runners", usesRealServer ? "No runner." : "build.example.com"],
      ["Export", "export", "Open Decisions"],
    ] as const) {
      await nav(page).getByRole("link", { name: label, exact: true }).click();
      await page.waitForURL(`**/${path}`);
      await page.getByRole("heading", { level: 2, name: label }).waitFor();
      await page.getByText(content, { exact: false }).first().waitFor();
      await expect.poll(() => page.getByLabel("Ask Moebius").count()).toBe(0);
    }
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
