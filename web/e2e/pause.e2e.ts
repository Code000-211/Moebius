/**
 * Stopping agents from where a person looks for it, in a real browser against the fake server: Fleet's header pausing
 * and resuming all agents (and following a pause made elsewhere), the palette finding "pause", "resume" and "stop", the
 * Stop of a Fleet row, the count that separates working from waiting, and a chat's Stop, Rename and Delete in its head.
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

const post = (path: string) =>
  context.request.post(`${server.url}${path}`, { headers: { "X-Moebius": "1" }, data: {} });

describe("pausing all agents", () => {
  it.skipIf(usesRealServer)("pauses and resumes from Fleet's header", async () => {
    await fresh();
    await page.goto(`${server.url}/fleet`);
    await page.getByRole("button", { name: "Pause all agents" }).click();
    await page.getByRole("status").filter({ hasText: "All agents are paused" }).waitFor();
    await page.getByRole("button", { name: "Resume all agents" }).click();
    await page.getByRole("button", { name: "Pause all agents" }).waitFor();
    await page.getByText("All agents are paused").waitFor({ state: "detached" });
    const calls = server.pagesA?.requests.map((r) => r.path).filter((p) => /pause|resume/.test(p));
    expect(calls).toEqual(["/v1/runs/pause", "/v1/runs/resume"]);
  });

  it("follows a pause made by another client", async () => {
    await fresh();
    await page.goto(`${server.url}/fleet`);
    await page.getByRole("button", { name: "Pause all agents" }).waitFor();
    expect((await post("/v1/runs/pause")).ok()).toBe(true);
    await page.getByRole("button", { name: "Resume all agents" }).waitFor();
    await page.getByRole("status").filter({ hasText: "paused by @owner" }).waitFor();
    expect((await post("/v1/runs/resume")).ok()).toBe(true);
    await page.getByRole("button", { name: "Pause all agents" }).waitFor();
  });

  it("is offered by the palette under pause, resume and stop", async () => {
    await fresh();
    await page.goto(`${server.url}/fleet`);
    await page.getByRole("button", { name: "Pause all agents" }).waitFor();
    await page.keyboard.press("Control+k");
    await page.getByRole("dialog").waitFor();
    await page.getByPlaceholder("Search…").fill("stop");
    await page.getByRole("option", { name: "Pause all agents" }).waitFor();
    await page.getByRole("option", { name: "Stop a run" }).waitFor();
    await page.getByPlaceholder("Search…").fill("pause");
    await page.getByRole("option", { name: "Pause all agents" }).click();
    await page.getByRole("button", { name: "Resume all agents" }).waitFor();
    await page.keyboard.press("Control+k");
    await page.getByPlaceholder("Search…").fill("resume");
    await page.getByRole("option", { name: "Resume all agents" }).click();
    await page.getByRole("button", { name: "Pause all agents" }).waitFor();
  });
});

describe.skipIf(usesRealServer)("stopping from Fleet", () => {
  it("stops one run with the Stop of its row, and counts waiting chats apart", async () => {
    await fresh();
    server.chat?.seed({ status: "running", title: "Write the rule" });
    server.chat?.seed({ status: "waiting", title: "Your turn one" });
    server.chat?.seed({ status: "waiting", title: "Your turn two" });
    await page.goto(`${server.url}/fleet`);
    await expect.poll(() => page.getByTestId("list-row").count()).toBe(4);
    const row = page.getByTestId("list-row").filter({ hasText: "Write the rule" });
    await row.hover();
    await page.getByRole("button", { name: "Stop Write the rule" }).click();
    await page.getByRole("status").filter({ hasText: "Stopping 1 run." }).waitFor();
    await expect.poll(() => page.getByTestId("list-row").count()).toBe(3);
  });
});

describe.skipIf(usesRealServer)("a chat's head", () => {
  it("offers Stop, Rename and Delete by name", async () => {
    await fresh();
    const id = server.chat?.seed({ status: "running", title: "Head chat" }) as number;
    await page.goto(`${server.url}/chat/${id}`);
    const head = page.getByTestId("chat-actions");
    await head.getByRole("button", { name: "Stop" }).click();
    await head.getByRole("button", { name: "Stop" }).waitFor({ state: "detached" });
    await head.getByRole("button", { name: "Rename" }).click();
    await page.getByRole("textbox", { name: "Name of the chat" }).fill("Renamed chat");
    await page.getByRole("dialog").getByRole("button", { name: "Rename" }).click();
    await page.getByTestId("chat-title").getByText("Renamed chat").waitFor();
    await head.getByRole("button", { name: "Delete" }).click();
    await page.getByRole("dialog").getByRole("button", { name: "Delete" }).click();
    await page.waitForURL(`${server.url}/chat`);
  });
});
