/**
 * A column of 1500 cards in a real browser: it draws only the cards in view, the last one is reachable by scrolling, a
 * card can still be dragged to another column and chosen in a range across cards that are not drawn, and the card being
 * dragged stays in the page when it is scrolled out of view. The fake server has the data, so these tests are skipped
 * against a real server.
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

const column = (name: string) => page.locator(`section[aria-label="${name}"]`);
const card = (number: number) => page.getByTestId(`card-${number}`);
/** The column's own scroller. */
const scroller = (name: string) => column(name).locator("[data-virtuoso-scroller]");
const scrollToEnd = (name: string) =>
  scroller(name).evaluate((node) => {
    node.scrollTop = node.scrollHeight;
  });
const spoken = async () => (await page.locator("[id^=DndLiveRegion]").textContent()) ?? "";

/** Opens a board whose Backlog has 1500 more cards; returns their numbers. */
async function openBoard(): Promise<number[]> {
  server = await startServer();
  if (!server.board) throw new Error("this test needs the fake server");
  const seeded = server.board.seedCards(1500, "Backlog");
  context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  await page.goto(`${server.url}/projects/1`);
  await page.getByRole("heading", { name: "demo", level: 1 }).waitFor();
  await card(2).waitFor();
  return seeded;
}

const drawn = (name: string) =>
  column(name)
    .getByTestId(/^card-\d+$/)
    .count();

describe("a column of 1500 cards", () => {
  it.skipIf(usesRealServer)("draws fewer than 100 cards and reaches the last", async () => {
    const seeded = await openBoard();
    const last = seeded[seeded.length - 1] as number;
    expect(await drawn("Backlog")).toBeLessThan(100);
    expect(await drawn("Backlog")).toBeGreaterThan(3);
    expect(await card(last).count()).toBe(0);
    await scrollToEnd("Backlog");
    await card(last).waitFor();
    expect(await drawn("Backlog")).toBeLessThan(100);
    // The other columns kept their place.
    expect(await scroller("Not started").evaluate((node) => node.scrollTop)).toBe(0);
  });

  it.skipIf(usesRealServer)("drags a card to another column", async () => {
    await openBoard();
    await card(2).focus();
    await page.keyboard.press("Space");
    await expect.poll(spoken).toContain("is over the column");
    await page.evaluate(() => new Promise((resolve) => setTimeout(resolve, 0)));
    await page.keyboard.press("ArrowRight");
    await expect.poll(spoken).toContain("is over the column Not started");
    await page.keyboard.press("Space");
    await column("Not started").getByTestId("card-2").waitFor();
    expect(server.board?.requests.at(-1)).toMatchObject({
      method: "POST",
      path: "/v1/cards/2/approve",
    });
  });

  it.skipIf(usesRealServer)("chooses a range over cards that are not drawn", async () => {
    const seeded = await openBoard();
    const last = seeded[seeded.length - 1] as number;
    await card(2).hover();
    await page.getByRole("checkbox", { name: "Choose #2", exact: true }).click();
    await scrollToEnd("Backlog");
    await card(last).hover();
    await page
      .getByRole("checkbox", { name: `Choose #${last}`, exact: true })
      .click({ modifiers: ["Shift"] });
    const chosen = page.getByRole("toolbar", { name: "Chosen" }).getByRole("status");
    // Every Backlog card from the first to the last, whether it is drawn or not.
    await chosen.filter({ hasText: /1,?501 cards chosen/ }).waitFor();
    expect(await drawn("Backlog")).toBeLessThan(100);
  });

  it.skipIf(usesRealServer)(
    "keeps the dragged card in the page when it is scrolled away",
    async () => {
      await openBoard();
      await card(2).focus();
      await page.keyboard.press("Space");
      await expect.poll(spoken).toContain("is over the column");
      await scrollToEnd("Backlog");
      const dragged: Locator = page.locator('[data-testid="card-2"]');
      await expect.poll(() => dragged.count()).toBe(1);
      await page.keyboard.press("Escape");
      // Let go, and the card is back in its row at the top.
      await scroller("Backlog").evaluate((node) => {
        node.scrollTop = 0;
      });
      await column("Backlog").getByTestId("card-2").waitFor();
      expect(await dragged.count()).toBe(1);
    },
  );
});
