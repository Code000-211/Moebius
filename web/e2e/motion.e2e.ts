/**
 * Motion in a real browser: a board card does not move or change its shadow under the pointer, no animation runs
 * forever on a page with a running card, and with the system's preference or the switch in Appearance no element of
 * a page has a transition left (the switch survives a reload).
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
  const context = await browser.newContext({
    reducedMotion,
    viewport: { width: 1280, height: 720 },
  });
  const page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  await page.goto(`${server.url}/board/${await boardWithCard(page)}`);
  await cardOf(page).waitFor();
  return page;
}

/** The first card of the board. */
const cardOf = (page: Page) => page.locator("[data-testid^='card-']").first();

/** The look of a card that hover could change. */
const look = (el: Element) => {
  const style = getComputedStyle(el);
  return { transform: style.transform, shadow: style.boxShadow };
};

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

describe("motion", () => {
  it("leaves a board card as it is under the pointer", async () => {
    const page = await board("no-preference");
    const card = cardOf(page);
    const before = await card.evaluate(look);
    // The board moves a new card to its column within a few seconds, from under the pointer, so the pointer follows
    // the card; a card that changed would not settle on the same look.
    await card.hover();
    await expect.poll(async () => card.evaluate(look)).toEqual(before);
    expect(before.transform).toBe("none");
    await page.context().close();
  });

  it("runs no animation forever", async () => {
    const page = await board("no-preference");
    const forever = await page.evaluate(
      () =>
        document
          .getAnimations()
          .filter((a) => a.effect?.getComputedTiming().iterations === Number.POSITIVE_INFINITY)
          .length,
    );
    expect(forever).toBe(0);
    await page.context().close();
  });

  it("keeps the routine transitions without a request for less", async () => {
    const page = await board("no-preference");
    expect((await page.evaluate(moving)).length).toBeGreaterThan(0);
    await page.context().close();
  });

  it("leaves no transition on the page when the system asks for less", async () => {
    const page = await board("reduce");
    expect(await page.evaluate(moving)).toEqual([]);
    await page.context().close();
  });

  it("leaves no transition when the switch is on, also after a reload", async () => {
    const page = await board("no-preference");
    await page.goto(`${server.url}/appearance`);
    await page.getByRole("switch", { name: "Reduce motion" }).click();
    expect(await page.evaluate(moving)).toEqual([]);
    await page.reload();
    await page.getByRole("switch", { name: "Reduce motion" }).waitFor();
    expect(await page.evaluate(() => document.documentElement.dataset.reduceMotion)).toBe("true");
    expect(await page.evaluate(moving)).toEqual([]);
    await page.getByRole("switch", { name: "Reduce motion" }).click();
    expect(
      await page.evaluate(() => document.documentElement.dataset.reduceMotion),
    ).toBeUndefined();
    await page.context().close();
  });
});
