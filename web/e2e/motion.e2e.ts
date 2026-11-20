/**
 * Reduced motion in a real browser: the quiet standard moves things only in answer to a person (a card lifts, a menu
 * opens with a small scale), and a person who asks the system for less motion gets none. With
 * `prefers-reduced-motion: reduce` no element of a page has a transition left, and a card does not lift on hover;
 * without it, the same card does.
 */
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type Browser, chromium, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { startServer, type TestServer, usesRealServer } from "./server";

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

/** The folders of the projects a real server was given, removed after each test. */
const directories: string[] = [];

afterEach(async () => {
  await server.stop().catch(() => {});
  for (const directory of directories.splice(0))
    rmSync(directory, { recursive: true, force: true });
});

/**
 * The board to open, with a card on it: the fake's first, or, on a real server, a project made for the test with one
 * issue on its board.
 */
async function boardWithCard(page: Page): Promise<number> {
  if (!usesRealServer) return 1;
  const directory = mkdtempSync(join(tmpdir(), "moebius-e2e-repo-"));
  directories.push(directory);
  const headers = { "X-Moebius": "1" };
  const made = await page.request.post(`${server.url}/v1/repositories`, {
    headers,
    data: { name: "demo", url: directory },
  });
  expect(made.ok(), await made.text()).toBe(true);
  const { id } = (await made.json()) as { id: number };
  const issue = await page.request.post(`${server.url}/v1/repositories/${id}/issues`, {
    headers,
    data: { title: "Lift me", body: "" },
  });
  expect(issue.ok(), await issue.text()).toBe(true);
  return id;
}

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
  await page.goto(`${server.url}/board/${await boardWithCard(page)}`);
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
    // The board moves the new card to the column it belongs in within a few seconds, from under the pointer, so the
    // pointer follows the card until it lifts.
    await expect
      .poll(async () => {
        await card.hover();
        return card.evaluate((el) => getComputedStyle(el).transform);
      })
      .not.toBe("none");
    await page.context().close();
  });
});
