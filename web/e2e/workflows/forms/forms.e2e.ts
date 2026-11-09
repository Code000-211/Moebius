/**
 * The forms of the code, condition, for-each and merge nodes in a real browser: the fields a kind reports, the
 * expressions typed into them, the JSON a save would write, and the refusal on the node when a required parameter is
 * missing. The fake server answers the same parameter checks the real one makes, so none of this needs a real server.
 */
import { type Browser, type BrowserContext, chromium, type Locator, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import type { WorkflowDefinition } from "../../../src/api/types/workflows";
import { pickOption } from "../../controls";
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

/**
 * The graph the file holds, with one node of each kind this card's forms cover, joined into it, and a second
 * condition node so a form that keeps state can be checked against the node chosen after the one it started on.
 */
function seed(graph: WorkflowDefinition): void {
  graph.nodes.push(
    { id: "shape", kind: "code", params: { source: "return input" }, position: { x: 120, y: 260 } },
    {
      id: "route",
      kind: "condition",
      params: { ports: [{ name: "pass", when: "input.ok" }], fallback: "other" },
      position: { x: 420, y: 260 },
    },
    {
      id: "audit",
      kind: "condition",
      params: { ports: [{ name: "flag", when: "input.flagged" }], fallback: "done" },
      position: { x: 720, y: 520 },
    },
    {
      id: "join",
      kind: "merge",
      params: { a: "test", b: "publish" },
      position: { x: 720, y: 260 },
    },
    {
      id: "per-file",
      kind: "for_each",
      params: { rejoin: "publish", max_elements: 10 },
      position: { x: 420, y: 520 },
    },
  );
  graph.edges.push(
    { from: "trigger", to: "shape", port: "out" },
    { from: "shape", to: "test", port: "out" },
    { from: "trigger", to: "route", port: "out" },
    { from: "trigger", to: "audit", port: "out" },
    { from: "route", to: "test", port: "pass" },
    { from: "route", to: "publish", port: "other" },
    { from: "audit", to: "test", port: "flag" },
    { from: "audit", to: "publish", port: "done" },
    { from: "test", to: "join", port: "out" },
    { from: "publish", to: "join", port: "out" },
    { from: "trigger", to: "per-file", port: "out" },
    { from: "per-file", to: "shape", port: "each" },
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
    .toBe(9);
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

describe.skipIf(usesRealServer)("the forms of the nodes that shape a graph", () => {
  it("saves what the code form shows, and refuses it without a source", async () => {
    await openKinds();
    await card("shape").click();
    await shows(inspector(), "Source", "Saved JSON");

    const source = inspector().getByRole("textbox", { name: "Source" });
    await source.fill("");
    await save();
    const refusal = 'node "shape": invalid parameters for node kind "code": "source" is required';
    await shows(card("shape"), refusal);
    await shows(state(), "Unsaved changes");

    const typed = "return { tag = input.tag }";
    await source.fill(typed);
    await shows(preview(), typed);
    await save();
    await shows(state(), "Saved as");
    expect(
      fake()
        .graphs.get("release")
        ?.nodes.find((node) => node.id === "shape")?.params,
    ).toEqual({ source: typed });
  });

  it("saves what the condition form shows, and refuses a port without an expression", async () => {
    await openKinds();
    await card("route").click();
    await shows(inspector(), "Ports", "Fallback", "Saved JSON");

    const when = inspector().getByRole("textbox", { name: "Port 1 when" });
    await when.fill("");
    await save();
    const refusal =
      'node "route": invalid parameters for node kind "condition": port "pass" needs an expression';
    await shows(card("route"), refusal);
    await shows(state(), "Unsaved changes");

    await when.fill('input.status == "ok"');
    await expect
      .poll(async () => inspector().getByRole("textbox", { name: "Fallback" }).inputValue())
      .toBe("other");
    await shows(
      preview(),
      '"name": "pass"',
      '"when": "input.status == \\"ok\\""',
      '"fallback": "other"',
    );
    await save();
    await shows(state(), "Saved as");
    // The card takes the ports the form now shows, not only the ones the kind started with.
    await expect
      .poll(() => card("route").locator('[title="Leaves by pass"]').count(), { timeout: 20_000 })
      .toBe(1);
    await expect
      .poll(() => card("route").locator('[title="Leaves by other"]').count(), { timeout: 20_000 })
      .toBe(1);
    expect(
      fake()
        .graphs.get("release")
        ?.nodes.find((node) => node.id === "route")?.params,
    ).toEqual({
      ports: [{ name: "pass", when: 'input.status == "ok"' }],
      fallback: "other",
    });
  });

  it("shows the ports of the condition node chosen last, not the one chosen before it", async () => {
    await openKinds();
    const name = inspector().getByRole("textbox", { name: "Port 1 name" });
    const when = inspector().getByRole("textbox", { name: "Port 1 when" });
    const fallback = inspector().getByRole("textbox", { name: "Fallback" });

    await card("route").click();
    await expect.poll(() => name.inputValue()).toBe("pass");

    // The condition form keeps its rows as state, so it must be built again for another node rather than reused.
    await card("audit").click();
    await expect.poll(() => name.inputValue()).toBe("flag");
    await expect.poll(() => when.inputValue()).toBe("input.flagged");
    await expect.poll(() => fallback.inputValue()).toBe("done");
  });

  it("saves what the merge form shows, and refuses a missing side", async () => {
    await openKinds();
    await card("join").click();
    await shows(inspector(), "Side A", "Side B", "Saved JSON");

    await pickOption(page, page.getByRole("button", { name: "Side A" }), "Choose a node");
    await save();
    const refusal =
      'node "join": invalid parameters for node kind "merge": "a" must name the node whose value arrives by port "a"';
    await shows(card("join"), refusal);
    await shows(state(), "Unsaved changes");

    await pickOption(page, page.getByRole("button", { name: "Side A" }), "shape");
    await pickOption(page, page.getByRole("button", { name: "Side B" }), "test");
    await inspector().getByRole("switch", { name: "Both sides" }).click();
    await shows(preview(), '"a": "shape"', '"b": "test"', '"both": true');
    await save();
    await shows(state(), "Saved as");
    expect(
      fake()
        .graphs.get("release")
        ?.nodes.find((node) => node.id === "join")?.params,
    ).toEqual({
      a: "shape",
      b: "test",
      both: true,
    });
  });

  it("saves what the for-each form shows, and refuses it without a rejoin", async () => {
    await openKinds();
    await card("per-file").click();
    await shows(inspector(), "Rejoins at", "Most elements", "Saved JSON");

    await pickOption(page, page.getByRole("button", { name: "Rejoins at" }), "Choose a node");
    await save();
    const refusal =
      'node "per-file": invalid parameters for node kind "for_each": "rejoin" must name the node where the sub-path rejoins';
    await shows(card("per-file"), refusal);
    await shows(state(), "Unsaved changes");

    await pickOption(page, page.getByRole("button", { name: "Rejoins at" }), "test");
    await inspector().getByRole("spinbutton", { name: "Most elements" }).fill("3");
    await shows(preview(), '"rejoin": "test"', '"max_elements": 3');
    await save();
    await shows(state(), "Saved as");
    expect(
      fake()
        .graphs.get("release")
        ?.nodes.find((node) => node.id === "per-file")?.params,
    ).toEqual({ rejoin: "test", max_elements: 3 });
  });
});
