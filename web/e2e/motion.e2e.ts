/**
 * Reduced motion in a real browser: the quiet standard moves things only in answer to a person (a card lifts, a menu
 * opens with a small scale), and a person who asks the system for less motion gets none. With
 * `prefers-reduced-motion: reduce` no element of a page has a transition left, and a card does not lift on hover;
 * without it, the same card does.
 */
import { type Browser, chromium, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { startServer, type TestServer } from "./server";

let browser: Browser;
let server: TestServer;

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
});

/**
 * The board of the repository of a server of its own, since a `?login=` link signs in once, with motion as the person
 * asked for it.
 */
async function board(reducedMotion: "reduce" | "no-preference"): Promise<Page> {
  server = await startServer();
  const context = await browser.newContext({ reducedMotion });
  const page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  await page.goto(`${server.url}/board/1`);
  await page.locator(".lift").first().waitFor();
  return page;
}

/** Every element, and its pseudo-elements, that still has a transition of some length. */
function moving(): string[] {
  const found: string[] = [];
  for (const el of Array.from(document.querySelectorAll("*"))) {
    for (const pseudo of [null, "::before", "::after"]) {
      const style = getComputedStyle(el, pseudo);
      const lengths = style.transitionDuration.split(",").map((each) => Number.parseFloat(each));
      if (lengths.some((length) => length > 0)) {
        found.push(`${el.tagName.toLowerCase()}${pseudo ?? ""} ${style.transitionDuration}`);
      }
    }
  }
  return found;
}

describe("reduced motion", () => {
  it("leaves no transition on the page and lifts no card", async () => {
    const page = await board("reduce");
    expect(await page.evaluate(moving)).toEqual([]);
    const card = page.locator(".lift").first();
    await card.hover();
    expect(await card.evaluate((el) => getComputedStyle(el).transform)).toBe("none");
    await page.context().close();
  });

  it("keeps the routine transitions and the lift otherwise", async () => {
    const page = await board("no-preference");
    expect((await page.evaluate(moving)).length).toBeGreaterThan(0);
    const card = page.locator(".lift").first();
    await card.hover();
    await expect.poll(() => card.evaluate((el) => getComputedStyle(el).transform)).not.toBe("none");
    await page.context().close();
  });
});
