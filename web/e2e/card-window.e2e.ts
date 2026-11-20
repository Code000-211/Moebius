/**
 * The card as a centred window over any page, in a real browser: it opens over a dimmed Inbox, Fleet or board without
 * moving the page (the board's lanes keep their width to the pixel), a reload with `?card=` opens it again, closing
 * drops the parameter and gives the focus back to what opened it, Escape closes only the topmost layer, and the form of
 * a new issue is a centred solid window over a dimmed page without blur. The fake server has the data.
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

async function open(path: string) {
  server = await startServer();
  context = await browser.newContext({ viewport: { width: 1280, height: 720 } });
  page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  await page.goto(`${server.url}${path}`);
}

const window = () => page.getByRole("dialog", { name: "Card inspector" });
const search = () => new URL(page.url()).search;

describe.skipIf(usesRealServer)("the card window", () => {
  it("opens over the Inbox, which stays behind the scrim, and sets ?card on /inbox", async () => {
    await open("/inbox");
    const list = page.getByTestId("inbox-item").first();
    await list.waitFor();
    await page.getByTestId("inbox-item").filter({ hasText: "Speed up the build" }).click();
    await page
      .getByTestId("inbox-decision")
      .getByRole("button", { name: "Open live view" })
      .click();
    await window().waitFor();
    expect(new URL(page.url()).pathname).toBe("/inbox");
    expect(new URLSearchParams(search()).get("card")).toBe("5");
    // The Inbox is still there behind it, dimmed by a scrim that does not blur.
    expect(await list.isVisible()).toBe(true);
    const scrim = page.locator('[data-state="open"].bg-scrim');
    await scrim.waitFor();
    expect(await scrim.evaluate((el) => getComputedStyle(el).backdropFilter)).toBe("none");
    // Centred, solid.
    const box = (await window().boundingBox()) as { x: number; width: number };
    expect(Math.abs(box.x + box.width / 2 - 640)).toBeLessThanOrEqual(2);
    expect(await window().evaluate((el) => getComputedStyle(el).backgroundColor)).not.toContain(
      "rgba(0, 0, 0, 0)",
    );
  });

  it("opens over Fleet from a running card", async () => {
    await open("/fleet");
    await page.getByTestId("list-row").first().waitFor();
    await page.getByTestId("list-row").filter({ hasText: "Add a feature" }).first().click();
    await window().waitFor();
    expect(new URL(page.url()).pathname).toBe("/fleet");
    expect(search()).toMatch(/^\?card=\d+$/);
  });

  it("does not move the board: the lanes keep their width to the pixel, and the header does not wrap", async () => {
    await open("/board/1");
    const lanes = page.getByRole("main").getByTestId("scroll-fades");
    await lanes.waitFor();
    const width = () => lanes.evaluate((el) => el.getBoundingClientRect().width);
    const before = await width();
    const header = page.locator("h1");
    const heightBefore = ((await header.boundingBox()) as { height: number }).height;
    await page.getByTestId("card-3").click();
    await window().waitFor();
    expect(await width()).toBe(before);
    expect(((await header.boundingBox()) as { height: number }).height).toBe(heightBefore);
    // The header stands on one line: its title and its buttons share a row.
    const title = (await header.boundingBox()) as { y: number; height: number };
    const button = (await page
      .locator("button", { hasText: "New issue" })
      .first()
      .boundingBox()) as {
      y: number;
    };
    expect(Math.abs(button.y - title.y)).toBeLessThan(title.height + 8);
    await page.keyboard.press("Escape");
    await window().waitFor({ state: "detached" });
    expect(await width()).toBe(before);
  });

  it("drops ?card on Escape and on the close icon, and returns the focus to the card", async () => {
    await open("/board/1");
    const card = page.getByTestId("card-3");
    await card.click();
    await window().waitFor();
    expect(search()).toBe("?card=3");
    await page.keyboard.press("Escape");
    await window().waitFor({ state: "detached" });
    expect(search()).toBe("");
    await expect
      .poll(() =>
        page.evaluate(() =>
          (document.activeElement as HTMLElement)
            ?.closest("[data-testid]")
            ?.getAttribute("data-testid"),
        ),
      )
      .toBe("card-3");

    await card.click();
    await window().getByRole("button", { name: "Close the inspector" }).click();
    await window().waitFor({ state: "detached" });
    expect(search()).toBe("");
    await expect
      .poll(() =>
        page.evaluate(() =>
          (document.activeElement as HTMLElement)
            ?.closest("[data-testid]")
            ?.getAttribute("data-testid"),
        ),
      )
      .toBe("card-3");
  });

  it("opens the same card again on reload, from any page", async () => {
    await open("/board/1?card=3");
    await window().getByRole("heading", { name: "Document the API" }).waitFor();
    await page.reload();
    await window().getByRole("heading", { name: "Document the API" }).waitFor();
    await page.goto(`${server.url}/fleet?card=3`);
    await window().getByRole("heading", { name: "Document the API" }).waitFor();
  });

  it("leaves the card open when Escape closes a dialog over it, and C opens the new issue window over it", async () => {
    await open("/board/1?card=3");
    await window().waitFor();
    await window().getByRole("button", { name: "Edit issue" }).click();
    const edit = page.getByRole("dialog", { name: "Edit #3" });
    await edit.waitFor();
    await page.keyboard.press("Escape");
    await edit.waitFor({ state: "detached" });
    await window().waitFor();
    await window().getByRole("heading", { name: "Document the API" }).click();
    await page.keyboard.press("c");
    await page.getByRole("dialog", { name: "New issue" }).waitFor();
    await page.keyboard.press("Escape");
    await page.getByRole("dialog", { name: "New issue" }).waitFor({ state: "detached" });
    await window().waitFor();
  });

  it("draws the form of a new issue as a centred solid window over a dimmed page, without blur", async () => {
    await open("/board/1");
    await page.getByRole("heading", { name: "demo", level: 1 }).waitFor();
    await page.keyboard.press("c");
    const form = page.getByRole("dialog", { name: "New issue" });
    await form.waitFor();
    const box = (await form.boundingBox()) as { x: number; width: number };
    expect(Math.abs(box.x + box.width / 2 - 640)).toBeLessThanOrEqual(2);
    expect(await form.evaluate((el) => getComputedStyle(el).backdropFilter)).toBe("none");
    expect(await form.evaluate((el) => getComputedStyle(el).backgroundColor)).not.toContain(
      "rgba(0, 0, 0, 0)",
    );
    const scrim = page.locator('[data-state="open"].bg-scrim');
    expect(await scrim.evaluate((el) => getComputedStyle(el).backdropFilter)).toBe("none");
    expect(Number(await scrim.evaluate((el) => getComputedStyle(el).opacity))).toBeGreaterThan(0);
  });
});
