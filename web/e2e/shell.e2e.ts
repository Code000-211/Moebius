/**
 * The shell in a real browser: one panel floating on the faintly tinted canvas, with the sidebar lying on the canvas
 * beside it, 6px of air above, below and at the right, round at all four corners, a hairline for its edge and no
 * shadow, and no strip above it or texture under it (`docs/design-web.md`). Then the properties pane inside the panel,
 * there only on a page that has properties, with the Moebius agent opened over it and closed again; and the desktop
 * app's window buttons, which are drawn once, inside the panel.
 */
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

afterEach(async () => {
  await server.stop().catch(() => {});
});

const VIEW = { width: 1440, height: 900 };

/**
 * The Inbox of a server of its own, since a `?login=` link signs in once, in a theme of its own, with the shell drawn.
 */
async function inbox(colorScheme: "dark" | "light"): Promise<Page> {
  server = await startServer();
  const context = await browser.newContext({
    colorScheme,
    viewport: VIEW,
    reducedMotion: "reduce",
  });
  const page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  await page.goto(`${server.url}/inbox`);
  await page.locator('[data-surface="page"] h1').first().waitFor();
  await expect.poll(() => page.locator("html").getAttribute("data-theme")).toBe(colorScheme);
  return page;
}

interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}

async function box(page: Page, selector: string): Promise<Box> {
  const found = await page.locator(selector).boundingBox();
  if (!found) throw new Error(`${selector} drew no box`);
  return found;
}

describe("the shell", () => {
  for (const theme of ["light", "dark"] as const) {
    it(`floats one panel on the canvas in ${theme}, with a hairline edge and no shadow`, async () => {
      const page = await inbox(theme);
      const surface = await box(page, '[data-surface="page"]');
      const nav = await box(page, 'nav[aria-label="Pages"]');
      expect(surface.y).toBe(6);
      expect(VIEW.height - (surface.y + surface.height)).toBe(6);
      expect(VIEW.width - (surface.x + surface.width)).toBe(6);
      expect(surface.x).toBe(nav.x + nav.width);
      const read = await page.evaluate(() => {
        const panel = document.querySelector('[data-surface="page"]') as HTMLElement;
        const probe = (name: string) => {
          const el = document.createElement("div");
          el.style.color = `var(${name})`;
          document.body.append(el);
          const colour = getComputedStyle(el).color;
          el.remove();
          return colour;
        };
        const style = getComputedStyle(panel);
        const canvas = getComputedStyle(panel.parentElement as HTMLElement);
        return {
          radius: style.borderRadius,
          line: style.borderTopColor,
          lineWidth: style.borderTopWidth,
          shadow: style.boxShadow,
          background: style.backgroundColor,
          canvasBackground: canvas.backgroundColor,
          canvasTexture: getComputedStyle(panel.parentElement as HTMLElement, "::after").content,
          canvasPadding: canvas.paddingTop,
          hairline: probe("--hairline"),
          panelColour: probe("--panel"),
          canvasColour: probe("--canvas"),
        };
      });
      expect(read.radius).toBe("10px");
      expect(read.lineWidth).toBe("1px");
      expect(read.line).toBe(read.hairline);
      expect(read.shadow).toBe("none");
      expect(read.background).toBe(read.panelColour);
      expect(read.canvasBackground).toBe(read.canvasColour);
      expect(read.canvasBackground).not.toBe(read.background);
      // No texture layer under the window, and no strip of padding above the panel but the 6px of air.
      expect(read.canvasTexture).toBe("none");
      expect(read.canvasPadding).toBe("6px");
      // The sidebar has no surface of its own: the canvas shows through it.
      const sidebar = await page
        .locator('nav[aria-label="Pages"]')
        .evaluate((el) => getComputedStyle(el).backgroundColor);
      expect(sidebar).toBe("rgba(0, 0, 0, 0)");
      expect(await page.locator(".pt-9").count()).toBe(0);
      await page.context().close();
    });
  }
});

/**
 * The theme the built app starts in. Light is the default: dark only when the system asks for it or a person chose it,
 * and a stored choice outranks the system. Each case reads `data-theme` and the `--canvas` it resolves to, so a theme
 * whose attribute is right but whose tokens are not is caught too.
 */
describe("the theme the app starts in", () => {
  const CANVAS = { light: "#f6f6f8", dark: "#0f0f10" } as const;

  async function started(
    colorScheme: "light" | "dark",
    stored?: "light" | "dark",
  ): Promise<{ theme: string | null; canvas: string }> {
    server = await startServer();
    const context = await browser.newContext({
      colorScheme,
      viewport: VIEW,
      reducedMotion: "reduce",
    });
    if (stored) {
      await context.addInitScript((value) => localStorage.setItem("moebius.theme", value), stored);
    }
    const page = await context.newPage();
    await page.goto(server.loginUrl);
    await page.getByRole("navigation", { name: "Pages" }).waitFor();
    const theme = await page.locator("html").getAttribute("data-theme");
    const canvas = await page.evaluate(() =>
      getComputedStyle(document.documentElement).getPropertyValue("--canvas").trim().toLowerCase(),
    );
    await context.close();
    return { theme, canvas };
  }

  it("is light with no stored choice and a light system", async () => {
    expect(await started("light")).toEqual({ theme: "light", canvas: CANVAS.light });
  });

  it("is dark with no stored choice and a dark system", async () => {
    expect(await started("dark")).toEqual({ theme: "dark", canvas: CANVAS.dark });
  });

  it("is dark with a stored dark choice, whatever the system asks for", async () => {
    expect(await started("light", "dark")).toEqual({ theme: "dark", canvas: CANVAS.dark });
  });
});

/** A page of a server of its own, signed in, with the shell drawn. */
async function signedIn(path: string): Promise<Page> {
  server = await startServer();
  const context = await browser.newContext({ viewport: VIEW, reducedMotion: "reduce" });
  const page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  await page.goto(`${server.url}${path}`);
  await page.locator('[data-surface="page"] h1').first().waitFor();
  return page;
}

const COLUMN = '[data-surface="right-column"]';

/**
 * The properties pane with a page's properties in it, and the Moebius agent lying over it: a workflow's definition has
 * properties (what starts it, its nodes, its last run), the Inbox has none and shows no pane. The properties are marked
 * before the agent opens, so a pane drawn again from nothing would be told by the mark being gone.
 */
describe.skipIf(usesRealServer)("the properties pane and the Moebius agent", () => {
  it("opens and closes the agent over a page with properties, which keep their state", async () => {
    const page = await signedIn("/workflows/release");
    const properties = page.locator(COLUMN).getByRole("region", { name: "Definition" });
    await properties.getByText("3 nodes").waitFor();
    // The pane lies inside the panel, flush with its top, right and bottom (the 1px edge of the panel apart).
    const panel = await box(page, '[data-surface="page"]');
    const pane = await box(page, COLUMN);
    expect(pane.width).toBe(272);
    expect(pane.y).toBe(panel.y + 1);
    expect(pane.x + pane.width).toBe(panel.x + panel.width - 1);
    expect(pane.y + pane.height).toBe(panel.y + panel.height - 1);
    await page.locator(COLUMN).getByRole("heading", { name: "Properties" }).waitFor();
    await properties.evaluate((element) => {
      element.dataset.kept = "yes";
    });
    const main = await box(page, "main");

    await page.keyboard.press("Control+j");
    const agent = page.getByRole("dialog", { name: "Moebius agent" });
    await agent.waitFor();
    // It lies over the pane, wider than it, flush with the panel's inside; the page keeps its width. The boxes are
    // read together, once the layout the sheet opened has settled.
    const layout = async () => {
      const surface = await box(page, '[data-surface="page"]');
      const over = await box(page, '[role="dialog"][aria-label="Moebius agent"]');
      return {
        overLeftOfColumn: over.x < (await box(page, COLUMN)).x,
        right: over.x + over.width - (surface.x + surface.width),
        top: over.y - surface.y,
        height: over.height - surface.height,
        main: await box(page, "main"),
      };
    };
    // Inside the panel's 1px edge.
    await expect
      .poll(layout)
      .toEqual({ overLeftOfColumn: true, right: -1, top: 1, height: -2, main });

    // "Properties" gives the place back, and they are as they were.
    await agent.getByRole("button", { name: "Properties" }).click();
    await agent.waitFor({ state: "hidden" });
    expect(await properties.getAttribute("data-kept")).toBe("yes");
    await properties.getByText("3 nodes").waitFor();

    // The shortcut opens it too, and Escape closes it.
    await page.keyboard.press("Control+j");
    await agent.waitFor();
    await page.keyboard.press("Escape");
    await agent.waitFor({ state: "hidden" });
    expect(await properties.getAttribute("data-kept")).toBe("yes");
    await page.context().close();
  });

  it("shows no right column on a page without properties, and still opens the agent", async () => {
    const page = await signedIn("/inbox");
    expect(await page.locator(COLUMN).isVisible()).toBe(false);
    expect(await page.locator(COLUMN).boundingBox()).toBeNull();
    // Nothing on the page offers to fold, show or ask: there is no rail and no row for the agent.
    expect(await page.getByRole("button", { name: "Show the properties" }).count()).toBe(0);
    expect(await page.getByRole("button", { name: "Ask Moebius" }).count()).toBe(0);
    await page.keyboard.press("Control+j");
    const agent = page.getByRole("dialog", { name: "Moebius agent" });
    await agent.waitFor();
    expect((await box(page, '[role="dialog"][aria-label="Moebius agent"]')).width).toBe(400);
    // Nothing to go back to here.
    expect(await agent.getByRole("button", { name: "Properties" }).count()).toBe(0);
    await agent.getByRole("button", { name: "Close the Moebius agent" }).click();
    await agent.waitFor({ state: "hidden" });
    expect(await page.locator(COLUMN).isVisible()).toBe(false);
    // No round button floats at the bottom right any more.
    expect(await page.getByRole("button", { name: "Open the floating chat" }).count()).toBe(0);
    await page.context().close();
  });

  it("has the pane without a fold button, and keeps it from page to page", async () => {
    const page = await signedIn("/workflows/release");
    const column = page.locator(COLUMN);
    await column.getByRole("region", { name: "Definition" }).getByText("3 nodes").waitFor();
    expect(await column.getByRole("button", { name: "Fold the properties" }).count()).toBe(0);
    // Away to the list, which has none, and back.
    await page.locator("main").getByRole("link", { name: "Workflows" }).click();
    await page.getByRole("heading", { name: "Workflows", level: 1 }).waitFor();
    await column.waitFor({ state: "hidden" });
    await page.getByRole("link", { name: "release" }).click();
    await page.getByRole("heading", { name: "release", level: 1 }).waitFor();
    expect((await box(page, COLUMN)).width).toBe(272);
    await page.context().close();
  });
});

/**
 * The desktop app's window has no system frame, so the web app draws the window's buttons: once, inside the panel,
 * when the app's bridge is there; and a strip that moves and closes the window on the screens outside the shell.
 */
describe.skipIf(usesRealServer)("the window buttons in the desktop app", () => {
  async function desktop(): Promise<Page> {
    server = await startServer();
    const context = await browser.newContext({ viewport: VIEW, reducedMotion: "reduce" });
    await context.addInitScript(() => {
      const nothing = async () => {};
      Object.defineProperty(window, "moebiusDesktop", {
        value: {
          minimize: () => {},
          toggleMaximize: () => {},
          close: () => {
            const counted = window as unknown as { closeCalls?: number };
            counted.closeCalls = (counted.closeCalls ?? 0) + 1;
          },
          servers: async () => [],
          switchServer: nothing,
          removeServer: nothing,
          renameServer: nothing,
          connect: nothing,
          connectSsh: nothing,
        },
      });
    });
    return context.newPage();
  }

  it("draws exactly one set, inside the panel, and no strip above it", async () => {
    const page = await desktop();
    await page.goto(server.loginUrl);
    await page.getByRole("navigation", { name: "Pages" }).waitFor();
    await page.goto(`${server.url}/inbox`);
    await page.locator('[data-surface="page"] h1').first().waitFor();
    for (const name of ["Minimize", "Maximize", "Close"]) {
      expect(await page.getByRole("button", { name, exact: true }).count(), name).toBe(1);
      expect(
        await page
          .locator('[data-surface="page"]')
          .getByRole("button", { name, exact: true })
          .count(),
        `${name} in the panel`,
      ).toBe(1);
    }
    expect(await page.locator(".pt-9").count()).toBe(0);
    expect(await page.getByTestId("window-bar").count()).toBe(0);
    // At the panel's top right, level with the header, which does not run under them.
    const panel = await box(page, '[data-surface="page"]');
    const controls = await box(page, '[data-testid="window-controls"]');
    expect(controls.y).toBeGreaterThanOrEqual(panel.y);
    expect(controls.x + controls.width).toBeLessThanOrEqual(panel.x + panel.width);
    expect(panel.x + panel.width - (controls.x + controls.width)).toBeLessThan(16);
    const header = await box(page, '[data-surface="page"] header');
    expect(header.y).toBeLessThan(controls.y + controls.height);
    await page.context().close();
  });

  it("keeps a way to move and close the window on the sign-in screen", async () => {
    const page = await desktop();
    await page.goto(`${server.url}/inbox`);
    await page.getByRole("button", { name: "Sign in" }).first().waitFor();
    const bar = page.getByTestId("window-bar");
    expect(await bar.getAttribute("data-tauri-drag-region")).not.toBeNull();
    await bar.getByRole("button", { name: "Close" }).click();
    expect(
      await page.evaluate(() => (window as unknown as { closeCalls?: number }).closeCalls),
    ).toBe(1);
    await page.context().close();
  });
});

/** A page of a server of its own at the given window width, signed in, with the shell drawn. */
async function signedInAt(path: string, width: number): Promise<Page> {
  server = await startServer();
  const context = await browser.newContext({
    viewport: { width, height: 800 },
    reducedMotion: "reduce",
  });
  const page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  await page.goto(`${server.url}${path}`);
  await page.locator('[data-surface="page"] h1').first().waitFor();
  return page;
}

describe("windows of 800 and 1280 px", () => {
  it("folds the sidebar to its rail below 1024 px by itself, and opens it for the moment on a click", async () => {
    const page = await signedInAt("/inbox", 800);
    const left = async () => (await box(page, '[data-surface="page"]')).x;
    expect(await left()).toBeLessThan(100);
    await page.getByRole("button", { name: "Expand the sidebar" }).click();
    await page.waitForFunction(
      () => (document.querySelector('[data-surface="page"]')?.getBoundingClientRect().x ?? 0) > 250,
    );
    await page.context().close();
    const wide = await signedInAt("/inbox", 1280);
    expect(await box(wide, '[data-surface="page"]').then((b) => b.x)).toBeGreaterThan(250);
    await wide.context().close();
  });

  it.skipIf(usesRealServer)(
    "keeps the description of a built-in pattern readable beside the Properties column",
    async () => {
      const page = await signedInAt("/rules", 1280);
      const description = page
        .getByRole("region", { name: "Built-in patterns" })
        .getByText("git push --force rewrites history on the remote");
      await description.waitFor();
      // About thirty characters a line at 14px is 200px; the page once left it 40px.
      expect((await description.boundingBox())?.width ?? 0).toBeGreaterThan(200);
      await page.context().close();
    },
  );

  it.skipIf(usesRealServer)(
    "insets a dialog of a page that did not set its own padding",
    async () => {
      const page = await signedInAt("/cycles", 1280);
      await page.getByRole("button", { name: "New cycle", exact: true }).first().click();
      const dialog = page.getByRole("dialog");
      await dialog.waitFor();
      const padding = await dialog.evaluate((el) => {
        const style = getComputedStyle(el);
        return [style.paddingLeft, style.paddingTop, style.rowGap];
      });
      expect(padding).toEqual(["24px", "24px", "16px"]);
      await page.context().close();
    },
  );
});
