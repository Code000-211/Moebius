/**
 * What a change shows, while events arrive without a pause (agents running): in the tab of the person who made it as
 * soon as the server answered, and in a second person's open board within about a second. The fake server
 * (`fake/board.ts`) answers and sends the events, so the tests are skipped against a real server.
 */
import { type Browser, type BrowserContext, chromium, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { startServer, type TestServer, usesRealServer } from "./server";

let browser: Browser;
let server: TestServer;
const contexts: BrowserContext[] = [];
let storm: ReturnType<typeof setInterval> | undefined;

beforeAll(async () => {
  browser = await chromium.launch({
    executablePath: process.env.MOEBIUS_E2E_CHROMIUM || undefined,
  });
});
afterAll(async () => {
  await browser.close();
});
afterEach(async () => {
  clearInterval(storm);
  for (const context of contexts.splice(0)) await context.close();
  await server.stop().catch(() => {});
});

/** A tab on the card `card` of the demo board; every tab shares the one signed-in session. */
async function open(card: number): Promise<Page> {
  if (contexts.length === 0) {
    const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
    contexts.push(context);
    const first = await context.newPage();
    await first.goto(server.loginUrl);
    await first.getByRole("navigation", { name: "Pages" }).waitFor();
    await first.close();
  }
  const page = await contexts[0]?.newPage();
  if (!page) throw new Error("no browser context");
  await page.goto(`${server.url}/projects/1?card=${card}`);
  // The properties are in a window of their own, opened from the menu of the card window.
  await page
    .locator("[data-card-window]")
    .getByRole("button", { name: `More actions for #${card}`, exact: true })
    .click();
  await page.getByRole("menuitem", { name: "Properties…" }).click();
  await page.getByRole("region", { name: `Properties of #${card}`, exact: true }).waitFor();
  return page;
}

describe.skipIf(usesRealServer)("a change while events arrive", () => {
  it("shows at once in the tab that made it, and soon in another person's", async () => {
    server = await startServer();
    const mine = await open(3);
    const theirs = await open(3);
    // Runs going on: an event every 50 ms that makes the snapshot stale.
    storm = setInterval(() => server.broadcast?.({ kind: "card_updated", data: {} }), 50);

    const priority = (page: Page, name: string | RegExp) =>
      page
        .getByRole("region", { name: "Properties of #3", exact: true })
        .getByRole("button", { name });
    await priority(mine, /^Priority/).click();
    await mine.getByRole("menuitemradio", { name: "High" }).click();
    await priority(mine, "Priority High").waitFor();
    // A second change: the panel is not one edit behind.
    await priority(mine, /^Priority/).click();
    await mine.getByRole("menuitemradio", { name: "Medium" }).click();
    await priority(mine, "Priority Medium").waitFor();

    await priority(theirs, "Priority Medium").waitFor({ timeout: 3000 });
    expect(await priority(theirs, /^Priority/).count()).toBe(1);
  });
});
