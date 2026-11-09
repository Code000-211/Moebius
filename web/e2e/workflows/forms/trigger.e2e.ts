/**
 * The forms of a workflow's trigger in a real browser: the values a manual run is given, the path and secret a webhook
 * is called at, and the cron a schedule fires on. A cron the server would not parse is shown as a problem and the
 * canvas will not save it.
 */
import { type Browser, type BrowserContext, chromium, type Locator, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import type { Trigger } from "../../../src/api/types/workflows";
import { offers, pickOption } from "../../controls";
import { startServer, type TestServer, usesRealServer } from "../../server";

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

const fake = () => {
  if (!server.workflows) throw new Error("this test needs the fake server");
  return server.workflows;
};

async function open(trigger: Trigger): Promise<void> {
  server = await startServer();
  const graph = fake().graphs.get("release");
  if (!graph) throw new Error("the fake server has no release workflow");
  graph.trigger = [trigger];
  context = await browser.newContext({ viewport: { width: 1600, height: 1000 } });
  page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  await page.goto(`${server.url}/workflows/release/canvas`);
  await page.getByTestId("canvas-page").waitFor();
  await card("trigger").waitFor();
}

function card(id: string): Locator {
  return page.locator(`[data-testid="canvas-node"][data-node-id="${id}"]`);
}

const state = () => page.getByTestId("canvas-state");
const inspector = () => page.getByTestId("canvas-inspector");
const preview = () => inspector().getByTestId("params-preview");
const saveButton = () => page.getByTestId("canvas-save");

async function shows(locator: Locator, ...phrases: string[]): Promise<void> {
  const words = async () => (await locator.textContent()) ?? "";
  for (const phrase of phrases) {
    await expect
      .poll(() => words().then((text) => text.includes(phrase)), {
        message: `did not find "${phrase}"`,
        timeout: 20_000,
        interval: 100,
      })
      .toBe(true);
  }
}

function savedTrigger(): Trigger | undefined {
  return fake().graphs.get("release")?.trigger[0];
}

describe.skipIf(usesRealServer)("the forms of the triggers", () => {
  it("blocks the save of a cron that does not parse, and saves one that does", async () => {
    await open({ kind: "schedule", params: { cron: "0 6 * * 1" } });
    await card("trigger").click();
    await shows(inspector(), "Cron", "0 6 * * 1");

    await page.getByRole("textbox", { name: "Cron" }).fill("not a cron");
    await shows(inspector(), "a cron has five fields");
    await expect
      .poll(() => saveButton().getAttribute("aria-disabled"), { timeout: 20_000 })
      .toBe("true");
    await expect
      .poll(() => saveButton().getAttribute("aria-describedby"), { timeout: 20_000 })
      .not.toBeNull();
    await shows(page.locator("body"), "a cron has five fields");

    // Sunday is day 0 or 7, and the server parses both.
    await page.getByRole("textbox", { name: "Cron" }).fill("0 0 * * 7");
    await shows(preview(), '"cron": "0 0 * * 7"');
    await expect
      .poll(() => saveButton().getAttribute("aria-disabled"), { timeout: 20_000 })
      .not.toBe("true");
    await saveButton().click();
    await shows(state(), "Saved as");
    expect(savedTrigger()).toEqual({ kind: "schedule", params: { cron: "0 0 * * 7" } });
  });

  it("saves what the webhook form shows: a path and the name of a secret", async () => {
    await open({ kind: "webhook", params: { path: "release", secret: "anthropic" } });
    await card("trigger").click();
    await shows(inspector(), "Path", "Secret", "release");

    const names = await offers(page, page.getByRole("button", { name: "Secret" }));
    expect(names).toEqual(["No secret", "anthropic"]);

    await page.getByRole("textbox", { name: "Path" }).fill("release-v2");
    await pickOption(page, page.getByRole("button", { name: "Secret" }), "anthropic");
    await shows(preview(), '"path": "release-v2"', '"secret": "anthropic"');
    await saveButton().click();
    await shows(state(), "Saved as");
    expect(savedTrigger()).toEqual({
      kind: "webhook",
      params: { path: "release-v2", secret: "anthropic" },
    });
  });

  it("saves what the manual form shows: the typed inputs a run is given", async () => {
    await open({ kind: "manual", params: {} });
    await card("trigger").click();
    await shows(inspector(), "Inputs", "Add input");

    await page.getByRole("button", { name: "Add input" }).click();
    await page.getByRole("textbox", { name: "Input 1 name" }).fill("tag");
    await page.getByRole("textbox", { name: "Input 1 default" }).fill("v1.4.0");
    await page.getByRole("switch", { name: "Input 1 required" }).click();
    await shows(preview(), '"name": "tag"', '"default": "v1.4.0"', '"required": true');
    await saveButton().click();
    await shows(state(), "Saved as");
    expect(savedTrigger()).toEqual({
      kind: "manual",
      params: { inputs: [{ name: "tag", type: "text", required: true, default: "v1.4.0" }] },
    });
  });
});
