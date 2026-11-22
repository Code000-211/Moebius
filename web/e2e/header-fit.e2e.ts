/**
 * A one-row page header never reaches past its right padding, which in the desktop app is the room of the window
 * buttons: at every window width the primary button drops its label first, then the tabs that do not fit move into
 * "More", and nothing overlaps the window buttons. The widths are swept from wide to narrow on a project's board, with
 * and without the project taking requests (Intake), and on an agent's page. The fake server and the desktop bridge are
 * as in shell.e2e.ts.
 */
import { type Browser, type BrowserContext, chromium, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { startServer, type TestServer, usesRealServer } from "./server";

let browser: Browser;
let server: TestServer;
let context: BrowserContext;

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

/** A signed-in page on `path`, with the desktop bridge when `desktop` is set. */
async function open(path: string, desktop: boolean): Promise<Page> {
  server = await startServer();
  context = await browser.newContext({
    viewport: { width: 1300, height: 800 },
    reducedMotion: "reduce",
  });
  if (desktop) {
    await context.addInitScript(() => {
      const nothing = async () => {};
      Object.defineProperty(window, "moebiusDesktop", {
        value: {
          minimize: () => {},
          toggleMaximize: () => {},
          close: () => {},
          servers: async () => [],
          switchServer: nothing,
          removeServer: nothing,
          renameServer: nothing,
          connect: nothing,
          connectSsh: nothing,
        },
      });
    });
  }
  const page = await context.newPage();
  page.setDefaultTimeout(8000);
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  await page.goto(`${server.url}${path}`);
  await page.locator("[data-page-header]").first().waitFor();
  return page;
}

/** Where the header's content ends, the left edge of what must stay clear of it, and whether the header scrolls. */
async function measure(page: Page) {
  return page.evaluate(() => {
    const header = document.querySelector<HTMLElement>("[data-page-header]");
    if (!header) throw new Error("no header");
    const rect = header.getBoundingClientRect();
    const end = (header.lastElementChild as HTMLElement).getBoundingClientRect().right;
    const controls = document
      .querySelector('[data-testid="window-controls"]')
      ?.getBoundingClientRect();
    return {
      end,
      limit: rect.right - Number.parseFloat(getComputedStyle(header).paddingRight),
      controlsLeft: controls?.left ?? null,
      overflow: header.scrollWidth - header.clientWidth,
      height: rect.height,
    };
  });
}

/** Sweeps the window from 1300 to 700 and checks the header at each width. */
async function sweep(page: Page, desktop: boolean) {
  for (let width = 1300; width >= 700; width -= 20) {
    await page.setViewportSize({ width, height: 800 });
    await expect
      .poll(async () => {
        const m = await measure(page);
        const clear =
          m.end <= m.limit + 0.5 && (m.controlsLeft === null || m.end <= m.controlsLeft);
        return clear && m.overflow <= 0 && m.height <= 48.5
          ? "fits"
          : JSON.stringify({ width, ...m });
      })
      .toBe("fits");
    if (desktop) expect((await measure(page)).controlsLeft).not.toBeNull();
  }
}

describe.skipIf(usesRealServer)("a one-row page header at every width", () => {
  for (const desktop of [true, false]) {
    const where = desktop ? "with the window buttons" : "without them";

    it(`keeps a project's board clear of its right padding ${where}`, async () => {
      const page = await open("/projects/1", desktop);
      await page.getByRole("tab", { name: "Board" }).waitFor();
      await sweep(page, desktop);
    });

    it(`keeps a project with Intake clear of its right padding ${where}`, async () => {
      const page = await open("/projects/1/settings", desktop);
      await page.getByRole("switch", { name: "Intake" }).click();
      await page.getByRole("switch", { name: "Intake", checked: true }).waitFor();
      await page.goto(`${server.url}/projects/1`);
      await page
        .getByRole("tab", { name: /^Intake/ })
        .first()
        .waitFor();
      await sweep(page, desktop);
    });
  }

  it("drops the label of New issue before moving a tab, and names the button still", async () => {
    const page = await open("/projects/1", true);
    await page.setViewportSize({ width: 1300, height: 800 });
    const button = page.locator("[data-page-header]").getByRole("button", { name: "New issue" });
    await expect.poll(() => button.textContent()).toContain("New issue");
    await expect.poll(() => page.getByTestId("tabs-more").count()).toBe(0);
    // Narrower: the label goes, the name and the tooltip stay, and every tab is still in the row.
    let width = 1300;
    while (((await button.textContent()) ?? "").includes("New issue") && width > 700) {
      width -= 10;
      await page.setViewportSize({ width, height: 800 });
      await expect
        .poll(async () => (await measure(page)).end <= (await measure(page)).limit + 0.5)
        .toBe(true);
    }
    expect(await button.getAttribute("title")).toBe("New issue");
    expect(await button.getAttribute("aria-label")).toBe("New issue");
    expect(width).toBeGreaterThan(700);
  });

  it("moves tabs into More and opens the page of one that was moved", async () => {
    const page = await open("/projects/1", true);
    await page.setViewportSize({ width: 700, height: 800 });
    const more = page.getByTestId("tabs-more");
    await more.waitFor();
    await more.click();
    const items = page.getByRole("menuitem");
    const names = await items.allTextContents();
    expect(names.length).toBeGreaterThan(0);
    const last = names[names.length - 1] ?? "";
    // The last tab is the first to go, and is reached from the menu like from the row.
    expect(last).toMatch(/Milestones|Intake/);
    await items.last().click();
    await page.waitForURL(/\/projects\/1\/(milestones|intake)$/);
    // The chosen tab, hidden, shows in More as the chosen one.
    await expect.poll(() => more.getAttribute("aria-selected")).toBe("true");
    await expect.poll(() => more.textContent()).toContain(last.replace(/\d+$/, "").trim());
    // Widening brings every tab back.
    await page.setViewportSize({ width: 1300, height: 800 });
    await expect.poll(() => page.getByTestId("tabs-more").count()).toBe(0);
    await page.getByRole("tab", { name: /^Milestones/ }).waitFor();
  });

  it("keeps an agent's page clear of its right padding", async () => {
    const page = await open("/agents/Coder", true);
    await page.getByRole("tab", { name: "Settings" }).waitFor();
    await sweep(page, true);
  });
});
