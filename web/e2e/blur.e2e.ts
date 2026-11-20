/**
 * Where the app blurs, in a real browser. A window, a sheet, the bar of a choice and the dimmed page behind a window
 * never blur; a menu and a tooltip blur when the root says `data-blur="on"` and are the opaque panel when it says
 * "off". Headless Chromium reports a software renderer, so the attribute is set by hand in both states.
 */
import { type Browser, type BrowserContext, chromium, type Locator, type Page } from "playwright";
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

const setBlur = (value: "on" | "off") =>
  page.evaluate((v) => {
    document.documentElement.dataset.blur = v;
  }, value);

const filterOf = (el: Locator) => el.evaluate((n) => getComputedStyle(n).backdropFilter);
/** The alpha of the background: 1 when the colour is written without one. */
const alphaOf = (el: Locator) =>
  el.evaluate((n) => {
    const colour = getComputedStyle(n).backgroundColor;
    const slash = /\/\s*([\d.]+%?)\s*\)/.exec(colour);
    const value = slash?.[1];
    if (value) return value.endsWith("%") ? Number.parseFloat(value) / 100 : Number(value);
    const rgba = /^rgba\(\s*[^,]+,[^,]+,[^,]+,\s*([\d.]+)\s*\)/.exec(colour);
    return rgba ? Number(rgba[1]) : 1;
  });

describe.skipIf(usesRealServer)("blur", () => {
  it("never blurs a window, a sheet, the bar of a choice or the scrim, and blurs menus and tooltips only when on", async () => {
    server = await startServer();
    if (!server.board) throw new Error("this test needs the fake server");
    const more = server.board.seedApprovals(2);
    context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
    page = await context.newPage();
    await page.goto(server.loginUrl);
    await page.getByRole("navigation", { name: "Pages" }).waitFor();
    await page.goto(`${server.url}/board/1`);
    await page.getByRole("heading", { name: "demo", level: 1 }).waitFor();
    await page.getByTestId(`card-${more[1]}`).waitFor();

    // A card a person has blocked carries a tag with a tooltip.
    await page.getByTestId("card-3").getByRole("button", { name: "More actions for #3" }).click();
    await page.getByRole("menuitem", { name: "Block…", exact: true }).click();
    const asking = page.getByRole("dialog", { name: "Block #3" });
    await asking.getByLabel("Reason").fill("wait for the design");
    await asking.getByRole("button", { name: "Block", exact: true }).click();
    await asking.waitFor({ state: "detached" });
    const tag = page.getByTestId("card-3").getByText("Blocked", { exact: true });
    await tag.waitFor();

    // The bar of a choice, and its menu.
    for (const n of [2, more[0]]) {
      await page.getByRole("checkbox", { name: `Choose #${n}` }).focus();
      await page.keyboard.press("Space");
    }
    const bar = page.getByRole("toolbar", { name: "Chosen" });
    await bar.waitFor();

    // The sheet; the tooltip is the one of the blocked tag.
    await page.getByRole("textbox", { name: "Ask Moebius" }).fill("Hello");
    await page.keyboard.press("Enter");
    const sheet = page.getByRole("complementary", { name: "Moebius agent" });
    await sheet.waitFor();

    for (const state of ["on", "off"] as const) {
      await setBlur(state);
      expect(await filterOf(bar)).toBe("none");
      expect(await filterOf(sheet)).toBe("none");
      expect(await alphaOf(sheet)).toBe(1);

      await bar.getByRole("button", { name: "Move" }).click();
      const menu = page.getByRole("menu");
      await menu.waitFor();
      if (state === "on") expect(await filterOf(menu)).not.toBe("none");
      else {
        expect(await filterOf(menu)).toBe("none");
        expect(await alphaOf(menu)).toBe(1);
      }
      await page.keyboard.press("Escape");
      await menu.waitFor({ state: "detached" });

      await tag.hover();
      const tip = page.locator(".glass.z-tip").first();
      await tip.waitFor();
      if (state === "on") expect(await filterOf(tip)).not.toBe("none");
      else {
        expect(await filterOf(tip)).toBe("none");
        expect(await alphaOf(tip)).toBe(1);
      }
      await page.mouse.move(5, 5);
    }

    // The new issue window and the scrim behind it.
    await sheet.getByRole("button", { name: "Close the Moebius agent" }).click();
    await sheet.waitFor({ state: "hidden" });
    await page.getByRole("button", { name: "New issue" }).click();
    const window = page.getByRole("dialog", { name: "New issue" });
    await window.waitFor();
    const scrim = page.locator(".bg-scrim");
    for (const state of ["on", "off"] as const) {
      await setBlur(state);
      expect(await filterOf(window)).toBe("none");
      expect(await alphaOf(window)).toBe(1);
      expect(await filterOf(scrim)).toBe("none");
    }
  });
});
