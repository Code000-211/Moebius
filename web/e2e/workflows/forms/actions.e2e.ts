/**
 * The forms of the http, agent, approval and publish nodes in a real browser: each form starts with the parameters the
 * file holds, a save writes what it shows, and the secret field offers the names the server holds and never a value.
 */
import { type Browser, type BrowserContext, chromium, type Locator, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import type { WorkflowDefinition } from "../../../src/api/types/workflows";
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

/** The graph the file holds, with one node of each kind this card's forms cover, joined into it. */
function seed(graph: WorkflowDefinition): void {
  graph.nodes.push(
    {
      id: "notify",
      kind: "http",
      params: { url: "https://example.com/hook", method: "POST", secret: "anthropic" },
      position: { x: 120, y: 260 },
    },
    {
      id: "ask",
      kind: "agent",
      params: { agent: "Coder", prompt: "Build the release", output: "summary" },
      position: { x: 420, y: 260 },
    },
    {
      id: "gate",
      kind: "approval",
      params: { prompt: "Publish 1.4.0?", timeout_seconds: 3600 },
      position: { x: 720, y: 260 },
    },
    {
      id: "ship",
      kind: "publish",
      params: { title: "Release 1.4.0", body: "What changed." },
      position: { x: 720, y: 520 },
    },
  );
  graph.edges.push(
    { from: "trigger", to: "notify", port: "out" },
    { from: "trigger", to: "ask", port: "out" },
    { from: "trigger", to: "gate", port: "out" },
    { from: "trigger", to: "ship", port: "out" },
  );
}

async function openKinds(): Promise<void> {
  server = await startServer();
  const graph = fake().graphs.get("release");
  if (!graph) throw new Error("the fake server has no release workflow");
  seed(graph);
  context = await browser.newContext({ viewport: { width: 1600, height: 1000 } });
  page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  await page.goto(`${server.url}/workflows/release/canvas`);
  await page.getByTestId("canvas-page").waitFor();
  await expect
    .poll(() => page.locator('[data-testid="canvas-node"]').count(), {
      message: "the seeded graph never drew",
      timeout: 20_000,
      interval: 100,
    })
    .toBe(8);
}

function card(id: string): Locator {
  return page.locator(`[data-testid="canvas-node"][data-node-id="${id}"]`);
}

const state = () => page.getByTestId("canvas-state");
const inspector = () => page.getByTestId("canvas-inspector");
const preview = () => inspector().getByTestId("params-preview");

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

async function save(): Promise<void> {
  await page.getByTestId("canvas-save").click();
}

function paramsOf(id: string): Record<string, unknown> | undefined {
  return fake()
    .graphs.get("release")
    ?.nodes.find((node) => node.id === id)?.params;
}

describe.skipIf(usesRealServer)("the forms of the nodes that act", () => {
  it("saves what the http form shows, and names a secret without ever showing its value", async () => {
    await openKinds();
    await card("notify").click();
    await shows(inspector(), "URL", "Method", "Secret", "https://example.com/hook");

    const options = await offers(page, page.getByRole("button", { name: "Secret" }));
    expect(options).toEqual(["No secret", "anthropic"]);

    await page.getByRole("textbox", { name: "URL" }).fill("https://example.com/v2/hook");
    await pickOption(page, page.getByRole("button", { name: "Method" }), "PUT");
    await pickOption(page, page.getByRole("button", { name: "Secret" }), "anthropic");
    await shows(preview(), '"url": "https://example.com/v2/hook"', '"method": "PUT"');
    await save();
    await shows(state(), "Saved as");
    expect(paramsOf("notify")).toEqual({
      url: "https://example.com/v2/hook",
      method: "PUT",
      secret: "anthropic",
    });
  });

  it("saves what the agent form shows: its agent, prompt and declared output", async () => {
    await openKinds();
    await card("ask").click();
    await shows(inspector(), "Agent", "Prompt", "Output", "Build the release");

    await page.getByRole("textbox", { name: "Prompt" }).fill("Publish the release");
    await page.getByRole("textbox", { name: "Output" }).fill("notes");
    await shows(preview(), '"prompt": "Publish the release"', '"output": "notes"');
    await save();
    await shows(state(), "Saved as");
    expect(paramsOf("ask")).toEqual({
      agent: "Coder",
      prompt: "Publish the release",
      output: "notes",
    });
  });

  it("saves what the approval form shows: its prompt and its deadline", async () => {
    await openKinds();
    await card("gate").click();
    await shows(inspector(), "Prompt", "Deadline", "Publish 1.4.0?");

    await expect
      .poll(() => card("gate").locator('[title="Leaves by failed"]').count(), { timeout: 20_000 })
      .toBe(1);
    await page.getByRole("textbox", { name: "Prompt" }).fill("Ship 1.4.0?");
    await page.getByRole("spinbutton", { name: "Deadline" }).fill("7200");
    await shows(preview(), '"prompt": "Ship 1.4.0?"', '"timeout_seconds": 7200');
    await save();
    await shows(state(), "Saved as");
    expect(paramsOf("gate")).toEqual({ prompt: "Ship 1.4.0?", timeout_seconds: 7200 });
  });

  it("saves what the publish form shows: its title and its body", async () => {
    await openKinds();
    await card("ship").click();
    await shows(inspector(), "Title", "Body", "Release 1.4.0");

    await page.getByRole("textbox", { name: "Title" }).fill("Release 1.4.1");
    await page.getByRole("textbox", { name: "Body" }).fill("A patch release.");
    await shows(preview(), '"title": "Release 1.4.1"', '"body": "A patch release."');
    await save();
    await shows(state(), "Saved as");
    expect(paramsOf("ship")).toEqual({ title: "Release 1.4.1", body: "A patch release." });
  });

  it("tells a person a publish node needs its body before it saves", async () => {
    await openKinds();
    await card("ship").click();
    await shows(inspector(), "Title", "Body");

    await page.getByRole("textbox", { name: "Body" }).fill("");
    await shows(inspector(), "A publish node needs its body.");

    await page.getByRole("textbox", { name: "Body" }).fill("What changed in this release.");
    await save();
    await shows(state(), "Saved as");
  });
});
