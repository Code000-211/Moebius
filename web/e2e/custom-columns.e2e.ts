/**
 * A project with columns of its own, in a real browser: the board shows them between In review and Ready to merge, a
 * card can be dragged to one (and is refused with the reason, as on a built-in column), the list layout groups by them,
 * the project's settings list them read-only with their scripts, and a custom column looks like the built-in one it
 * counts as. The fake server's project has "Ready for testing" and "Being measured", which count as In review
 * (`fake/board.ts`).
 */
import { type Browser, type BrowserContext, chromium, type Locator, type Page } from "playwright";
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

async function open(path: string, heading = "demo") {
  server = await startServer();
  context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  page = await context.newPage();
  page.setDefaultTimeout(8000);
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  server.board?.seedCustomColumns();
  await page.goto(`${server.url}${path}`);
  await page.getByRole("heading", { name: heading, level: 1 }).waitFor();
}

const column = (name: string) => page.locator(`section[aria-label="${name}"]`);
const card = (number: number) => page.getByTestId(`card-${number}`);
const spoken = async () => (await page.locator("[id^=DndLiveRegion]").textContent()) ?? "";

/** Lifts a card with the keyboard, moves it `steps` columns to the right and drops it. */
async function drag(target: Locator, steps: number) {
  await target.focus();
  await page.keyboard.press("Space");
  await expect.poll(spoken).toContain("is over the column");
  await page.evaluate(() => new Promise((resolve) => setTimeout(resolve, 0)));
  for (let i = 0; i < steps; i++) {
    const before = await spoken();
    await page.keyboard.press("ArrowRight");
    await expect.poll(spoken).not.toBe(before);
  }
  await page.keyboard.press("Space");
}

describe.skipIf(usesRealServer)("custom board columns", () => {
  it("shows the project's columns in the order it names them, with their cards", async () => {
    await open("/projects/1");
    const names = await page
      .locator("section[aria-label]")
      .evaluateAll((sections) => sections.map((section) => section.getAttribute("aria-label")));
    expect(names.filter((name) => name !== null)).toEqual([
      "Backlog",
      "Not started",
      "In progress",
      "In review",
      "Ready for testing",
      "Being measured",
      "Ready to merge",
      "Done",
    ]);
    await column("Ready for testing").getByTestId("card-8").waitFor();
    await column("Ready for testing").getByTestId("card-9").waitFor();
    await column("Being measured").getByTestId("card-10").waitFor();
  });

  it("refuses a drop on a custom column that nobody decides, with the reason, and moves nothing", async () => {
    await open("/projects/1");
    const before = server.board?.requests.length ?? 0;
    // Not started -> Ready for testing: three columns to the right.
    await drag(card(3), 3);
    await page
      .getByRole("alert")
      .filter({ hasText: "Cannot move #3 to Ready for testing: Nobody decides this card now" })
      .waitFor();
    await column("Not started").getByTestId("card-3").waitFor();
    expect(server.board?.requests.length).toBe(before);
  });

  it("lifts a card of a custom column to the next column with the keyboard drag", async () => {
    await open("/projects/1");
    // Ready for testing -> Being measured is a move the board makes itself: refused with the reason.
    await drag(card(8), 1);
    await page.getByRole("alert").filter({ hasText: "Cannot move #8 to Being measured" }).waitFor();
    await column("Ready for testing").getByTestId("card-8").waitFor();
  });

  it("moves a card in when the server moves it there", async () => {
    await open("/projects/1");
    const moved = server.board?.moveCard(3, "Being measured");
    if (!moved || !server.broadcast) throw new Error("this test needs the fake server");
    server.broadcast(moved);
    await column("Being measured").getByTestId("card-3").waitFor();
  });

  it("groups the list layout by the custom columns, in order", async () => {
    await open("/projects/1/list");
    const body = page.getByRole("main");
    await body.getByText("Ready for testing", { exact: true }).first().waitFor();
    const text = (await body.innerText()).replaceAll(/\s+/g, " ");
    const at = (name: string) => text.indexOf(name);
    expect(at("In review")).toBeLessThan(at("Ready for testing"));
    expect(at("Ready for testing")).toBeLessThan(at("Being measured"));
    expect(at("Being measured")).toBeLessThan(at("Ready to merge"));
  });

  it("offers the custom columns in the Column filter", async () => {
    await open("/projects/1?column=Being%20measured");
    await column("Being measured").getByTestId("card-10").waitFor();
    expect(await page.locator("section[aria-label]").count()).toBe(1);
  });

  it("lists the custom columns read-only in the project's settings, with the script of each", async () => {
    await open("/projects/1/settings/columns/Being%20measured?repository=1", "demo › Settings");
    const list = page.getByRole("list", { name: "Custom columns" });
    await list.waitFor();
    const rows = await list.getByRole("listitem").allInnerTexts();
    expect(rows.map((row) => row.replaceAll(/\s+/g, " "))).toEqual([
      "Ready for testing counts as In review testing.luau",
      "Being measured counts as In review measured.luau",
    ]);
    // Read-only: the list has no control, and says where the columns are defined.
    expect(await list.getByRole("button").count()).toBe(0);
    await page.getByText("kanban.toml").first().waitFor();
    // The script of a custom column opens in the same editor as the others.
    await page.getByRole("textbox", { name: "Script of Being measured" }).waitFor();
    // A column the project defines never "runs the built-in script": its script is the project's file, or its class's.
    const body = page.getByRole("main");
    await body.getByText("runs the script of In review").waitFor();
    expect(await body.innerText()).not.toContain("built in");
    await body.getByText("A built-in column runs the built-in script").waitFor();
  });

  it("keeps the built-in wording for a built-in column's script", async () => {
    await open("/projects/1/settings/columns/Backlog?repository=1", "demo › Settings");
    await page.getByRole("main").getByText("built in", { exact: false }).first().waitFor();
  });
});
