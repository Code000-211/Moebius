/**
 * The shell in a real browser: the faintly tinted sidebar and the white page meeting at one hairline, the page filling
 * the window to its edges, and no strip above it or texture under it (`docs/design-web.md`). Then the properties pane inside the page,
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
    it(`meets the tinted sidebar and the page at one hairline, filling the window, in ${theme}`, async () => {
      const page = await inbox(theme);
      const surface = await box(page, '[data-surface="page"]');
      const nav = await box(page, 'nav[aria-label="Pages"]');
      // The page fills the window to its edges: no air above, below or at the right, and it starts where the sidebar ends.
      expect(surface.y).toBe(0);
      expect(VIEW.height - (surface.y + surface.height)).toBe(0);
      expect(VIEW.width - (surface.x + surface.width)).toBe(0);
      expect(surface.x).toBe(nav.x + nav.width);
      const read = await page.evaluate(() => {
        const panel = document.querySelector('[data-surface="page"]') as HTMLElement;
        const nav = document.querySelector('nav[aria-label="Pages"]') as HTMLElement;
        const probe = (name: string) => {
          const el = document.createElement("div");
          el.style.color = `var(${name})`;
          document.body.append(el);
          const colour = getComputedStyle(el).color;
          el.remove();
          return colour;
        };
        const style = getComputedStyle(panel);
        const side = getComputedStyle(nav);
        return {
          radius: style.borderRadius,
          lineWidth: style.borderTopWidth,
          shadow: style.boxShadow,
          background: style.backgroundColor,
          sidebarBackground: side.backgroundColor,
          sidebarLine: side.borderRightColor,
          sidebarLineWidth: side.borderRightWidth,
          hairline: probe("--hairline"),
          panelColour: probe("--panel"),
          canvasColour: probe("--canvas"),
        };
      });
      // The page is the white panel, square and with no line or shadow of its own.
      expect(read.radius).toBe("0px");
      expect(read.lineWidth).toBe("0px");
      expect(read.shadow).toBe("none");
      expect(read.background).toBe(read.panelColour);
      // The sidebar is the faintly tinted canvas, and its right edge is the one hairline between them.
      expect(read.sidebarBackground).toBe(read.canvasColour);
      expect(read.sidebarBackground).not.toBe(read.background);
      expect(read.sidebarLineWidth).toBe("1px");
      expect(read.sidebarLine).toBe(read.hairline);
      await expect.poll(() => page.locator(".pt-9").count()).toBe(0);
      await page.context().close();
    });
  }

  it("draws a chosen row of the sidebar as a grey pill of 8px with no border or shadow", async () => {
    const page = await inbox("light");
    const row = page.locator('nav[aria-label="Pages"] a[aria-current="page"]').first();
    await row.waitFor();
    const read = await row.evaluate((el) => {
      const style = getComputedStyle(el);
      return {
        radius: style.borderRadius,
        border: style.borderTopWidth,
        shadow: style.boxShadow,
        height: el.getBoundingClientRect().height,
        background: style.backgroundColor,
      };
    });
    expect(read.radius).toBe("8px");
    expect(read.border).toBe("0px");
    expect(read.shadow).toBe("none");
    expect(read.height).toBe(36);
    expect(read.background).not.toBe("rgba(0, 0, 0, 0)");
    await page.context().close();
  });
});

/**
 * The theme the built app starts in. Light is the default: dark only when the system asks for it or a person chose it,
 * and a stored choice outranks the system. Each case reads `data-theme` and the `--canvas` it resolves to, so a theme
 * whose attribute is right but whose tokens are not is caught too.
 */
describe("the theme the app starts in", () => {
  const CANVAS = { light: "#f9f9fa", dark: "#0f0f10" } as const;

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
  await page.locator('[data-surface="page"]').waitFor();
  await page.goto(`${server.url}${path}`);
  await page.locator('[data-surface="page"] h1').first().waitFor();
  return page;
}

/**
 * The Moebius agent's panel: it opens at the panel's right over the same page, which keeps its state, and closes again.
 * The page's own facts (a workflow's steps) are in the page, so there is no pane of properties beside it.
 */
describe.skipIf(usesRealServer)("the Moebius agent", () => {
  const PANEL = { name: "Moebius agent" } as const;

  it("opens at the panel's right and closes again, the page kept", async () => {
    const page = await signedIn("/projects/1/workflows/release");
    const properties = page.getByRole("region", { name: /^Steps/ });
    await properties.getByText("3 steps").waitFor();
    const panel = await box(page, '[data-surface="page"]');
    await properties.evaluate((element) => {
      element.dataset.kept = "yes";
    });

    await page.keyboard.press("Control+j");
    const agent = page.getByRole("complementary", PANEL);
    await agent.waitFor();
    // The agent's panel lies inside the page, flush with its top, right and bottom (the page has no edge of its own).
    const over = await box(page, '[data-surface="agent-panel"]');
    expect(over.y).toBe(panel.y);
    expect(over.x + over.width).toBe(panel.x + panel.width);
    expect(over.y + over.height).toBe(panel.y + panel.height);

    await agent.getByRole("button", { name: "Close the Moebius agent" }).click();
    await agent.waitFor({ state: "hidden" });
    expect(await properties.getAttribute("data-kept")).toBe("yes");
    await properties.getByText("3 steps").waitFor();

    // The shortcut opens it too, and Escape closes it.
    await page.keyboard.press("Control+j");
    await agent.waitFor();
    await page.keyboard.press("Escape");
    await agent.waitFor({ state: "hidden" });
    expect(await properties.getAttribute("data-kept")).toBe("yes");
    await page.context().close();
  });

  const BUTTON = { name: "Moebius", exact: true } as const;

  it("opens from the header's button beside the page, and closes by the button, the close icon and Escape", async () => {
    const page = await signedIn("/inbox");
    const button = page.getByRole("button", BUTTON);
    await button.waitFor();
    expect(await button.getAttribute("aria-pressed")).toBe("false");
    const agent = page.getByRole("complementary", PANEL);
    const wide = await box(page, "main");
    await button.click();
    await agent.waitFor();
    expect(await button.getAttribute("aria-pressed")).toBe("true");
    // The panel is 344px wide beside the content, which is narrower by as much.
    expect((await box(page, '[data-surface="agent-panel"]')).width).toBe(344);
    expect((await box(page, "main")).width).toBe(wide.width - 344);
    // The focus is in the reply field.
    await page.getByPlaceholder("Reply to Moebius…").waitFor();
    expect(await page.evaluate(() => document.activeElement?.tagName)).toBe("TEXTAREA");
    await expect.poll(() => agent.locator("kbd").count()).toBe(0);

    await button.click();
    await agent.waitFor({ state: "hidden" });
    expect(await button.getAttribute("aria-pressed")).toBe("false");
    expect(await page.evaluate(() => document.activeElement?.textContent)).toBe("Moebius");

    await button.click();
    await agent.waitFor();
    await agent.getByRole("button", { name: "Close the Moebius agent" }).click();
    await agent.waitFor({ state: "hidden" });

    await button.click();
    await agent.waitFor();
    await page.keyboard.press("Escape");
    await agent.waitFor({ state: "hidden" });
    expect(await page.evaluate(() => document.activeElement?.textContent)).toBe("Moebius");
    await page.context().close();
  });

  it("opens and closes with Ctrl+J, and the button follows", async () => {
    const page = await signedIn("/inbox");
    const button = page.getByRole("button", BUTTON);
    const agent = page.getByRole("complementary", PANEL);
    await page.keyboard.press("Control+j");
    await agent.waitFor();
    expect(await button.getAttribute("aria-pressed")).toBe("true");
    await page.keyboard.press("Control+j");
    await agent.waitFor({ state: "hidden" });
    expect(await button.getAttribute("aria-pressed")).toBe("false");
    await page.context().close();
  });

  it("has the button in the header of pages, Settings among them, and no Ask Moebius field on any", async () => {
    const page = await signedIn("/inbox");
    for (const path of [
      "/inbox",
      "/my-issues",
      "/fleet",
      "/projects/1",
      "/wiki",
      "/projects/1/cycles",
      "/views",
      "/settings",
    ]) {
      await page.goto(`${server.url}${path}`);
      await page.locator('[data-surface="page"] h1').first().waitFor();
      await page.locator("header").getByRole("button", BUTTON).waitFor();
      await expect.poll(() => page.getByLabel("Ask Moebius").count()).toBe(0);
      await expect.poll(() => page.getByPlaceholder("Ask Moebius…").count()).toBe(0);
      await expect.poll(() => page.locator('[data-surface="agent-dock"]').count()).toBe(0);
    }
    await page.context().close();
  });

  it("lies over the page from the right in a window under 1000px, the button staying in the header", async () => {
    const page = await signedIn("/inbox");
    await page.setViewportSize({ width: 900, height: 700 });
    const button = page.getByRole("button", BUTTON);
    await button.waitFor();
    const before = await box(page, "main");
    await button.click();
    await page.getByRole("complementary", PANEL).waitFor();
    expect((await box(page, "main")).width).toBe(before.width);
    const over = await box(page, '[data-surface="agent-panel"]');
    const surface = await box(page, '[data-surface="page"]');
    expect(over.x + over.width).toBe(surface.x + surface.width);
    await expect.poll(() => button.isVisible()).toBe(true);
    expect(await button.getAttribute("aria-pressed")).toBe("true");
    await page.context().close();
  });
});

/**
 * The desktop app's window has no system frame, so the web app draws the window's buttons: once, inside the page,
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
      await expect.poll(() => page.getByRole("button", { name, exact: true }).count()).toBe(1);
      await expect
        .poll(() =>
          page.locator('[data-surface="page"]').getByRole("button", { name, exact: true }).count(),
        )
        .toBe(1);
    }
    await expect.poll(() => page.locator(".pt-9").count()).toBe(0);
    await expect.poll(() => page.getByTestId("window-bar").count()).toBe(0);
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
  await page.locator('[data-surface="page"]').waitFor();
  await page.goto(`${server.url}${path}`);
  await page.locator('[data-surface="page"] h1').first().waitFor();
  return page;
}

describe("windows of 800 and 1280 px", () => {
  const sidebar = (page: Page) => page.getByRole("navigation", { name: "Pages" });

  it.each([999, 900])("hides the sidebar at %i px until the button is pressed", async (width) => {
    const page = await signedInAt("/inbox", width);
    await expect.poll(() => sidebar(page).count()).toBe(0);
    expect((await box(page, '[data-surface="page"]')).x).toBeLessThan(40);
    await page.getByRole("button", { name: "Show the sidebar" }).click();
    await sidebar(page).waitFor();
    // Choosing a page closes it.
    await sidebar(page).getByRole("link", { name: "Fleet" }).click();
    await page.waitForURL(/\/fleet/);
    await sidebar(page).waitFor({ state: "detached" });
    // Escape and the scrim close it too.
    await page.getByRole("button", { name: "Show the sidebar" }).click();
    await sidebar(page).waitFor();
    await page.keyboard.press("Escape");
    await sidebar(page).waitFor({ state: "detached" });
    await page.getByRole("button", { name: "Show the sidebar" }).click();
    await page
      .getByRole("button", { name: "Close the sidebar" })
      .click({ position: { x: width - 20, y: 300 } });
    await sidebar(page).waitFor({ state: "detached" });
    await page.context().close();
  });

  it.each([1000, 1280])(
    "keeps the sidebar beside the page at %i px, without a button",
    async (width) => {
      const page = await signedInAt("/inbox", width);
      await sidebar(page).waitFor();
      await expect
        .poll(() => page.getByRole("button", { name: "Show the sidebar" }).count())
        .toBe(0);
      expect((await box(page, '[data-surface="page"]')).x).toBeGreaterThan(200);
      await page.context().close();
    },
  );

  it.skipIf(usesRealServer)(
    "keeps the description of a built-in pattern readable at the width of a laptop window",
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
      const page = await signedInAt("/projects/1/cycles", 1280);
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
