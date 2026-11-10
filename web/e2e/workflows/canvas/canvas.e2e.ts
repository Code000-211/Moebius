/**
 * The canvas in a real browser: the graph a workflow's file holds, a kind taken from the palette and joined to the
 * graph by its ports, a save that writes the graph as a commit and keeps where a node was moved to, a save the server
 * refuses shown on the node it names, a run picked and drawn over the graph, and the ask before leaving with changes
 * that are not saved.
 *
 * The graph library draws with elements of its own, so a check that is about the drawing names them —
 * `.react-flow__node`, `.react-flow__edge`, the viewport a pan and zoom move — while everything about the workflow is
 * read from the app's own cards and panels. The fake server answers the save and the runs and refuses a graph the way
 * the server's checks do, so none of this runs against a real one (MOEBIUS_E2E_SERVER).
 */
import { type Browser, type BrowserContext, chromium, type Locator, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
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

async function openCanvas(path = "/workflows/release/canvas") {
  server = await startServer();
  context = await browser.newContext({ viewport: { width: 1600, height: 1000 } });
  page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  await page.goto(`${server.url}${path}`);
  await page.getByTestId("canvas-page").waitFor();
  await atRest();
}

/**
 * Waits until the graph has come to rest: every card is where it will stay and the wires between them are drawn.
 * Until then the view is still fitting itself to the graph and a card is not yet under the pointer a test aims at.
 */
async function atRest(cards = 4, edges = 3): Promise<void> {
  let last = "";
  await expect
    .poll(
      async () => {
        const where = await resting();
        const same = where === last && (await page.locator(".react-flow__edge").count()) === edges;
        last = where;
        return same;
      },
      { message: "the graph never came to rest", timeout: 20_000, interval: 100 },
    )
    .toBe(true);
  expect(await page.locator('[data-testid="canvas-node"]').count()).toBe(cards);
}

async function resting(): Promise<string> {
  const boxes = await page.locator('[data-testid="canvas-node"]').evaluateAll((nodes) =>
    nodes
      .map((node) => {
        const r = node.getBoundingClientRect();
        return `${Math.round(r.x)},${Math.round(r.y)}`;
      })
      .join(";"),
  );
  return `${boxes}#${(await page.locator(".react-flow__viewport").getAttribute("style")) ?? ""}`;
}

afterEach(async () => {
  await context.close();
  await server.stop().catch(() => {});
});

const fake = () => {
  if (!server.workflows) throw new Error("this test needs the fake server");
  return server.workflows;
};

/** A card of the graph, by the node it stands for. */
function card(id: string): Locator {
  return page.locator(`[data-testid="canvas-node"][data-node-id="${id}"]`);
}

/** The port a card leaves by, or comes in by. */
function port(id: string, title: string): Locator {
  return card(id).locator(`[title="${title}"]`);
}

const state = () => page.getByTestId("canvas-state");
const wires = () => page.locator(".react-flow__edge");
const inspector = () => page.getByTestId("canvas-inspector");

const words = async (locator: Locator): Promise<string> => (await locator.textContent()) ?? "";

async function shows(locator: Locator, ...phrases: string[]): Promise<void> {
  for (const phrase of phrases) {
    await expect
      .poll(() => words(locator).then((text) => text.includes(phrase)), {
        message: `did not find "${phrase}"`,
        timeout: 20_000,
        interval: 100,
      })
      .toBe(true);
  }
}

async function thereAre(locator: Locator, count: number): Promise<void> {
  await expect.poll(() => locator.count(), { timeout: 20_000, interval: 100 }).toBe(count);
}

/**
 * Where a thing is, once the graph has stopped moving under it: the view fits itself when the graph is first drawn,
 * and a card measured after that is not where it was a moment before.
 */
async function box(locator: Locator) {
  let last = await locator.boundingBox();
  await expect
    .poll(
      async () => {
        const now = await locator.boundingBox();
        const settled = JSON.stringify(now) === JSON.stringify(last);
        last = now;
        return settled;
      },
      { message: "the graph kept moving", timeout: 20_000, interval: 100 },
    )
    .toBe(true);
  if (!last) throw new Error("a card is missing from the graph");
  return last;
}

/** Holds a port down, walks to another and lets go: the way a person joins two nodes. */
async function join(from: Locator, to: Locator) {
  await from.waitFor();
  await to.waitFor();
  const a = await box(from);
  const b = await box(to);
  await page.mouse.move(a.x + a.width / 2, a.y + a.height / 2);
  await page.mouse.down();
  await page.mouse.move(a.x + a.width / 2 + 30, a.y + a.height / 2 + 14, { steps: 6 });
  await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2, { steps: 12 });
  await page.mouse.up();
}

/** Carries a node across the graph and lets go where a person puts it. */
async function drag(id: string, dx: number, dy: number) {
  const where = await box(card(id));
  const from = { x: where.x + where.width / 2, y: where.y + 10 };
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(from.x + dx, from.y + dy, { steps: 12 });
  await page.mouse.up();
}

function kind(kind: string): Locator {
  return page.locator(`[data-testid="palette-kind"][data-kind="${kind}"]`);
}

describe.skipIf(usesRealServer)("the workflow canvas", () => {
  it("draws the graph the file holds, its start in front of it", async () => {
    await openCanvas();
    await shows(card("trigger"), "Trigger", "schedule");
    await shows(card("build"), "build", "agent");
    await shows(card("test"), "test");
    await thereAre(wires(), 3);
    await shows(state(), "Saved");

    // The corner controls move the view, which is how a graph wider than the window is looked over.
    const view = page.locator(".react-flow__viewport");
    const before = await view.getAttribute("style");
    await page.getByRole("button", { name: "Zoom In" }).click();
    await expect
      .poll(() => view.getAttribute("style"), { timeout: 20_000, interval: 100 })
      .not.toBe(before);
  });

  it("takes a kind from the palette and joins two nodes by their ports", async () => {
    await openCanvas();
    await kind("for_each").click();
    await shows(card("for_each"), "for_each");
    // The graph on the screen has changed; the file has not.
    expect(
      fake()
        .graphs.get("release")
        ?.nodes.map((node) => node.id),
    ).not.toContain("for_each");
    await shows(state(), "Unsaved changes");

    await join(port("publish", "Leaves by out"), port("for_each", "Comes in by in"));
    await thereAre(wires(), 4);
  });

  it("saves the graph as a commit, with where each node sits", async () => {
    await openCanvas();
    const moved = fake()
      .graphs.get("release")
      ?.nodes.find((node) => node.id === "test");
    const from = { ...moved?.position };
    await drag("test", -60, 130);
    await shows(state(), "Unsaved changes");
    await page.getByTestId("canvas-save").click();
    await shows(state(), "Saved as");

    const saved = fake()
      .graphs.get("release")
      ?.nodes.find((node) => node.id === "test");
    // Where it was moved to is what the file holds now: to the left of where it was, and below.
    expect(saved?.position.x ?? 0).toBeLessThan(from.x ?? 0);
    expect(saved?.position.y ?? 0).toBeGreaterThan(from.y ?? 0);
    // The save went through the route, and the file the read-only page shows was written with it.
    await page.getByRole("link", { name: "release" }).first().click();
    await page.getByRole("heading", { name: "release", level: 1 }).waitFor();
    await shows(page.locator("main"), "at = [");
  });

  it("shows a refused save on the node it names", async () => {
    await openCanvas();
    await kind("for_each").click();
    await page.getByTestId("canvas-save").click();

    // The server builds a node kind before it checks the graph, so its missing parameter is the first refusal.
    const refusal =
      'node "for_each": invalid parameters for node kind "for_each": "rejoin" must name the node where the sub-path rejoins';
    await shows(card("for_each"), refusal);
    await shows(page.getByTestId("canvas-problem"), refusal);
    await shows(state(), "Unsaved changes");

    // What the kind asks for is given, its sub-path is laid out and rejoins, and the same save goes through.
    await shows(inspector(), "Rejoins at");
    await pickOption(page, page.getByRole("button", { name: "Rejoins at" }), "publish");
    // The menu has to be gone before the pointer goes to a port, or it takes the first click.
    await atRest(5, 3);
    await join(card("trigger").locator(".canvas-handle"), port("for_each", "Comes in by in"));
    await join(port("for_each", "Leaves by each"), port("publish", "Comes in by in"));
    await thereAre(wires(), 5);
    await page.getByTestId("canvas-save").click();
    await shows(state(), "Saved as");
    expect(
      fake()
        .graphs.get("release")
        ?.nodes.map((node) => node.id),
    ).toContain("for_each");
  });

  it("keeps the panel on a node it renames, and refuses the trigger's name", async () => {
    await openCanvas();
    await card("test").click();
    await shows(inspector(), "test", "cargo test", "Delete node");

    // The panel follows the node to its new name instead of looking for the old one and finding nothing.
    const id = page.getByRole("textbox", { name: "Id" });
    await id.fill("check");
    await id.blur();
    await expect.poll(() => card("check").count(), { timeout: 20_000, interval: 100 }).toBe(1);
    await shows(inspector(), "check", "cargo test", "Delete node");
    expect(await words(inspector())).not.toContain("no longer on the graph");

    // The trigger's name is refused where it is typed; the node keeps the name it had.
    await page.getByRole("textbox", { name: "Id" }).fill("trigger");
    await page.getByRole("textbox", { name: "Id" }).blur();
    await shows(inspector(), "The trigger's id is reserved.");
    expect(await card("check").count()).toBe(1);
  });

  it("shows a run picked over the graph, with what each node was given", async () => {
    await openCanvas();
    await pickOption(page, page.getByRole("button", { name: "Run to show" }), "Run #41 · failed");

    await shows(card("build"), "Done");
    await shows(card("test"), "Failed");
    await shows(card("publish"), "Skipped");
    await shows(card("build"), "artifact");

    await card("test").click();
    await shows(inspector(), "cargo test --workspace", "a_card_moves failed");
  });

  it("asks before leaving with changes that are not saved", async () => {
    await openCanvas();
    await kind("for_each").click();
    await shows(state(), "Unsaved changes");

    await page.getByRole("link", { name: "release" }).first().click();
    const ask = page.getByRole("dialog", { name: "Leave with unsaved changes?" });
    await ask.waitFor();
    await ask.getByRole("button", { name: "Stay", exact: true }).click();
    expect(page.url()).toContain("/workflows/release/canvas");

    await page.getByRole("link", { name: "release" }).first().click();
    await ask.getByRole("button", { name: "Leave without saving" }).click();
    await page.getByRole("heading", { name: "release", level: 1 }).waitFor();
    expect(page.url()).not.toContain("/canvas");
    expect(
      fake()
        .graphs.get("release")
        ?.nodes.map((node) => node.id),
    ).not.toContain("for_each");
  });

  it("saves on the way out, and leaves once it is saved", async () => {
    await openCanvas();
    const before = {
      ...fake()
        .graphs.get("release")
        ?.nodes.find((node) => node.id === "test")?.position,
    };
    await drag("test", -60, 130);
    await shows(state(), "Unsaved changes");

    await page.getByRole("link", { name: "release" }).first().click();
    const ask = page.getByRole("dialog", { name: "Leave with unsaved changes?" });
    await ask.waitFor();
    await ask.getByRole("button", { name: "Save and leave" }).click();
    await page.getByRole("heading", { name: "release", level: 1 }).waitFor();
    expect(page.url()).not.toContain("/canvas");

    const saved = fake()
      .graphs.get("release")
      ?.nodes.find((node) => node.id === "test");
    expect(saved?.position.x ?? 0).toBeLessThan(before.x ?? 0);
    expect(saved?.position.y ?? 0).toBeGreaterThan(before.y ?? 0);
  });

  it("takes a node and a wire away", async () => {
    await openCanvas();
    await kind("for_each").click();
    await join(port("publish", "Leaves by out"), port("for_each", "Comes in by in"));
    await thereAre(wires(), 4);

    // The keyboard takes the chosen node away, the wires with it.
    await card("for_each").click();
    await page.keyboard.press("Delete");
    await expect.poll(() => card("for_each").count(), { timeout: 20_000, interval: 100 }).toBe(0);
    await thereAre(wires(), 3);

    // A wire chosen on the graph is taken away from the panel beside it.
    await wires().first().click();
    await page.getByTestId("delete-edge").click();
    await thereAre(wires(), 2);
  });
});
