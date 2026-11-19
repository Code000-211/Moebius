/**
 * The shell in a real browser, read from its pixels: a textured ground under the whole window, the sidebar on it with
 * no surface of its own, and the page as one surface with its right column, 6px of air above, below and at the right,
 * rounded at all four corners (ADR 0020). In dark the page is set apart by its lightness and a faint light edge alone,
 * because a wide shadow on the near-black ground shows as bands; so the ground in the air beside the page is the same
 * as the ground far from it. The grain stays on while the pixels are read: every reading is the mean of a strip, which
 * evens the grain out, and is compared with a strip of the same rows, which evens out the light from above. Then the
 * right column at work: a page's properties in it, folded and kept folded, and the Moebius agent opened over it and
 * closed again.
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

/** A picture of the window as the grey of each pixel, row by row, and a way to take the mean of a rectangle of it. */
async function picture(page: Page) {
  const png = (await page.screenshot()).toString("base64");
  const greys = await page.evaluate(async (data) => {
    const image = new Image();
    image.src = `data:image/png;base64,${data}`;
    await image.decode();
    const canvas = document.createElement("canvas");
    canvas.width = image.width;
    canvas.height = image.height;
    const context = canvas.getContext("2d");
    if (!context) throw new Error("no canvas");
    context.drawImage(image, 0, 0);
    const { data: rgba } = context.getImageData(0, 0, image.width, image.height);
    const out: number[] = [];
    for (let i = 0; i < rgba.length; i += 4) {
      out.push(((rgba[i] ?? 0) + (rgba[i + 1] ?? 0) + (rgba[i + 2] ?? 0)) / 3);
    }
    return { width: image.width, out };
  }, png);
  /** The mean grey of the pixels from `x0` to `x1` and `y0` to `y1`, both ends included. */
  return (x0: number, x1: number, y0: number, y1: number): number => {
    let sum = 0;
    let count = 0;
    for (let y = Math.round(y0); y <= Math.round(y1); y += 1) {
      for (let x = Math.round(x0); x <= Math.round(x1); x += 1) {
        sum += greys.out[y * greys.width + x] ?? 0;
        count += 1;
      }
    }
    return sum / count;
  };
}

describe("the shell", () => {
  it("lays the page on the ground with 6px of air above, below and at the right", async () => {
    const page = await inbox("dark");
    const surface = await box(page, '[data-surface="page"]');
    const nav = await box(page, 'nav[aria-label="Pages"]');
    expect(surface.y).toBe(6);
    expect(VIEW.height - (surface.y + surface.height)).toBe(6);
    expect(VIEW.width - (surface.x + surface.width)).toBe(6);
    expect(surface.x).toBe(nav.x + nav.width);
    const style = await page
      .locator('[data-surface="page"]')
      .evaluate((el) => getComputedStyle(el).borderRadius);
    expect(style).toBe("12px");
    // The sidebar has no surface of its own: the ground shows through it.
    const sidebar = await page
      .locator('nav[aria-label="Pages"]')
      .evaluate((el) => getComputedStyle(el).backgroundColor);
    expect(sidebar).toBe("rgba(0, 0, 0, 0)");
    await page.context().close();
  });

  for (const theme of ["dark", "light"] as const) {
    it(`draws the page and its right column as one surface in ${theme}`, async () => {
      const page = await inbox(theme);
      const read = (selector: string) =>
        page.locator(selector).evaluate((el) => {
          const style = getComputedStyle(el);
          return {
            background: style.backgroundColor,
            line: style.borderLeftColor,
            lineWidth: style.borderLeftWidth,
            shadow: style.boxShadow,
          };
        });
      const surface = await read('[data-surface="page"]');
      const column = await read('[data-surface="right-column"]');
      // The column has no colour of its own and no shadow: it is the page's surface, behind a soft line.
      expect(column.background).toBe("rgba(0, 0, 0, 0)");
      expect(column.shadow).toBe("none");
      expect(column.lineWidth).toBe("1px");
      const hairline = await page.evaluate(() => {
        const probe = document.createElement("div");
        probe.style.color = "var(--hairline)";
        document.body.append(probe);
        const colour = getComputedStyle(probe).color;
        probe.remove();
        return colour;
      });
      expect(column.line).toBe(hairline);
      expect(surface.background).not.toBe("rgba(0, 0, 0, 0)");

      // The column lies inside the page's box, flush with its top, right and bottom.
      const outer = await box(page, '[data-surface="page"]');
      const inner = await box(page, '[data-surface="right-column"]');
      expect(inner.y).toBe(outer.y);
      expect(inner.x + inner.width).toBe(outer.x + outer.width);
      expect(inner.y + inner.height).toBe(outer.y + outer.height);
      // Its head is the height of the page's head bar.
      const head = await box(page, '[data-surface="right-column"] [data-part="head"]');
      expect(head.height).toBe(44);

      // And it reads so: the column, below its head, is the grey of the page beside it, and both are lighter than the
      // ground.
      const mean = await picture(page);
      const rows = [inner.y + 200, inner.y + inner.height - 200] as const;
      const inColumn = mean(inner.x + 6, inner.x + inner.width - 6, ...rows);
      const onPage = mean(inner.x - 30, inner.x - 10, ...rows);
      expect(Math.abs(inColumn - onPage)).toBeLessThan(1.5);
      const ground = mean(outer.x + outer.width + 2, VIEW.width - 1, ...rows);
      expect(inColumn - ground).toBeGreaterThan(5);
      await page.context().close();
    });
  }

  it("shows no banding at the page's edge in dark, and no square edge under its corners", async () => {
    const page = await inbox("dark");
    const outer = await box(page, '[data-surface="page"]');
    const mean = await picture(page);
    const right = outer.x + outer.width;
    const bottom = outer.y + outer.height;
    const top = outer.y;

    // The right air, column by column past the 1px edge, against the ground at the far left of the same rows, in
    // the sidebar's own padding. A wide shadow would darken the air near the page and lighten it step by step away
    // from it; the ground beside the page is the ground far from it.
    const rows = [top + 40, top + 600] as const;
    const far = mean(0, 3, ...rows);
    for (let x = right + 2; x < VIEW.width; x += 1) {
      expect(
        Math.abs(mean(x, x, ...rows) - far),
        `the air ${x - right}px right of the page`,
      ).toBeLessThan(1.5);
    }
    // The air under the page, against the air under the sidebar, row by row.
    for (let y = bottom + 2; y < VIEW.height; y += 1) {
      const under = mean(outer.x + 40, outer.x + 600, y, y);
      const beside = mean(0, outer.x - 40, y, y);
      expect(Math.abs(under - beside), `the air ${y - bottom}px under the page`).toBeLessThan(1.5);
    }
    // Past each rounded corner the ground shows, not a square of the page's colour: a few pixels in the cut of the
    // corner are the ground of the air beside them.
    const corner = (x0: number, x1: number, y0: number, y1: number) => mean(x0, x1, y0, y1);
    const airTop = mean(right + 2, VIEW.width - 1, top, top + 2);
    expect(Math.abs(corner(right - 3, right - 1, top, top + 2) - airTop)).toBeLessThan(4);
    const sideTop = mean(0, 3, top, top + 2);
    const sideBottom = mean(0, 3, bottom - 3, bottom - 1);
    expect(Math.abs(corner(outer.x, outer.x + 2, top, top + 2) - sideTop)).toBeLessThan(4);
    expect(
      Math.abs(corner(outer.x, outer.x + 2, bottom - 3, bottom - 1) - sideBottom),
    ).toBeLessThan(4);
    await page.context().close();
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
 * The right column with a page's properties in it, and the Moebius agent lying over it: a workflow's definition has
 * properties (what starts it, its nodes, its last run), the Inbox has none. The properties are marked before the agent
 * opens, so a column drawn again from nothing would be told by the mark being gone.
 */
describe.skipIf(usesRealServer)("the right column and the Moebius agent", () => {
  it("opens and closes the agent over a page with properties, which keep their state", async () => {
    const page = await signedIn("/workflows/release");
    const properties = page.locator(COLUMN).getByRole("region", { name: "Definition" });
    await properties.getByText("3 nodes").waitFor();
    expect((await box(page, COLUMN)).width).toBe(272);
    await page.locator(COLUMN).getByRole("heading", { name: "Properties" }).waitFor();
    await properties.evaluate((element) => {
      element.dataset.kept = "yes";
    });
    const main = await box(page, "main");

    await page.getByRole("button", { name: "Ask Moebius" }).click();
    const agent = page.getByRole("dialog", { name: "Moebius agent" });
    await agent.waitFor();
    // It lies over the column, wider than it, flush with the page's top, right and bottom; the page keeps its width.
    const surface = await box(page, '[data-surface="page"]');
    const over = await box(page, '[role="dialog"][aria-label="Moebius agent"]');
    const column = await box(page, COLUMN);
    expect(over.x).toBeLessThan(column.x);
    expect(over.x + over.width).toBe(surface.x + surface.width);
    expect(over.y).toBe(surface.y);
    expect(over.height).toBe(surface.height);
    expect(await box(page, "main")).toEqual(main);

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
    expect(
      await page.evaluate(() => document.activeElement?.getAttribute("aria-label") ?? null),
    ).toBe("Ask Moebius");
    await page.context().close();
  });

  it("opens and closes the agent over a page without properties, whose column stays slim", async () => {
    const page = await signedIn("/inbox");
    const column = page.locator(COLUMN);
    expect((await box(page, COLUMN)).width).toBe(44);
    expect(await column.getByRole("button", { name: "Show the properties" }).isDisabled()).toBe(
      true,
    );
    await column.getByRole("button", { name: "Ask Moebius" }).click();
    const agent = page.getByRole("dialog", { name: "Moebius agent" });
    await agent.waitFor();
    expect((await box(page, '[role="dialog"][aria-label="Moebius agent"]')).width).toBe(400);
    // Nothing to go back to here.
    expect(await agent.getByRole("button", { name: "Properties" }).count()).toBe(0);
    await agent.getByRole("button", { name: "Close the Moebius agent" }).click();
    await agent.waitFor({ state: "hidden" });
    expect((await box(page, COLUMN)).width).toBe(44);
    // No round button floats at the bottom right any more.
    expect(await page.getByRole("button", { name: "Open the floating chat" }).count()).toBe(0);
    await page.context().close();
  });

  it("folds to a slim column with both buttons, and stays folded from page to page", async () => {
    const page = await signedIn("/workflows/release");
    const column = page.locator(COLUMN);
    const properties = column.getByRole("region", { name: "Definition" });
    await properties.getByText("3 nodes").waitFor();
    await column.getByRole("button", { name: "Fold the properties" }).click();
    expect((await box(page, COLUMN)).width).toBe(44);
    await properties.waitFor({ state: "hidden" });
    await column.getByRole("button", { name: "Show the properties" }).waitFor();
    await column.getByRole("button", { name: "Ask Moebius" }).waitFor();

    // Away to the list and back to the workflow: it is still folded.
    await page.locator("main").getByRole("link", { name: "Workflows" }).click();
    await page.getByRole("heading", { name: "Workflows", level: 1 }).waitFor();
    await page.getByRole("link", { name: "release" }).click();
    await page.getByRole("heading", { name: "release", level: 1 }).waitFor();
    expect((await box(page, COLUMN)).width).toBe(44);
    await column.getByRole("button", { name: "Show the properties" }).click();
    expect((await box(page, COLUMN)).width).toBe(272);
    await properties.getByText("3 nodes").waitFor();
    await page.context().close();
  });
});
