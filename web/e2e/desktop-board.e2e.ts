/**
 * The board in the desktop app's window at its default size (1280×800): the app draws its window buttons in the page's
 * panel and a rail of servers beside it, and a card's drawer is placed in the panel, so it stays clear of the buttons,
 * inside the panel and clear of the board's own controls. The bridge the app gives the page is faked.
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

interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}

const box = async (locator: ReturnType<Page["locator"]>) => (await locator.boundingBox()) as Box;

describe("the board in the desktop app's window", () => {
  it.skipIf(usesRealServer)(
    "keeps a card's drawer clear of the window's buttons, inside the page and off the board's controls",
    async () => {
      server = await startServer();
      context = await browser.newContext({ viewport: { width: 1280, height: 800 } });
      await context.addInitScript(() => {
        const nothing = async () => {};
        Object.defineProperty(window, "moebiusDesktop", {
          value: {
            minimize: () => {},
            toggleMaximize: () => {},
            close: () => {},
            servers: async () => [
              {
                id: "a",
                name: "127.0.0.1:7171",
                address: "http://127.0.0.1:7171",
                open: true,
                reachable: true,
                inbox: 0,
                items: 0,
                unread: 0,
              },
            ],
            switchServer: nothing,
            removeServer: nothing,
            renameServer: nothing,
            connect: nothing,
            connectSsh: nothing,
          },
        });
      });
      page = await context.newPage();
      await page.goto(server.loginUrl);
      await page.getByRole("navigation", { name: "Pages" }).waitFor();
      await page.getByTestId("rail").waitFor();
      await page.goto(`${server.url}/board/1?card=3`);
      const drawer = page.getByTestId("inspector-drawer");
      await drawer.waitFor();
      // The page's head sets its rows once the fonts are in: read the boxes again until they stop moving, then check
      // where things ended up (the assertions are on the settled layout, never on a moment before it).
      await page.evaluate(() => document.fonts.ready);
      const measure = async () => {
        const [bar, surface, shown, newIssue] = await Promise.all([
          box(page.getByTestId("window-controls")),
          box(page.locator('[data-surface="page"]')),
          box(drawer),
          box(page.getByRole("button", { name: "New issue" })),
        ]);
        return { bar, surface, shown, newIssue };
      };
      await expect
        .poll(
          async () => {
            const { shown, newIssue } = await measure();
            return (
              newIssue.x + newIssue.width <= shown.x ||
              shown.x + shown.width <= newIssue.x ||
              newIssue.y + newIssue.height <= shown.y ||
              shown.y + shown.height <= newIssue.y
            );
          },
          { message: "New issue is under the drawer", timeout: 10_000 },
        )
        .toBe(true);
      const { bar, surface, shown } = await measure();
      // Clear of the window's buttons, and inside the surface of the page, as it is in a browser.
      expect(
        shown.x + shown.width <= bar.x ||
          shown.y >= bar.y + bar.height ||
          shown.y + shown.height <= bar.y,
        "the drawer lies over the window's buttons",
      ).toBe(true);
      expect(shown.y).toBeGreaterThanOrEqual(surface.y - 0.5);
      expect(shown.y + shown.height).toBeLessThanOrEqual(surface.y + surface.height + 0.5);
      expect(shown.x + shown.width).toBeLessThanOrEqual(surface.x + surface.width + 0.5);
    },
  );
});
