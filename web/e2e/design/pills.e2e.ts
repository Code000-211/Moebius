/**
 * The standing guard against coloured state pills, which runs with `pnpm e2e` and so in CI. `findPills` (the detector
 * of the design audit) must find a pill built any way a pill can be built, must leave a banner, a neutral chip and a
 * tall tinted button alone, and must find nothing on the pages that once carried pills, in both themes.
 */
import { type Browser, chromium, type Page } from "playwright";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { startServer, type TestServer } from "../server";
import { findPills } from "./pills";

let browser: Browser;
let server: TestServer;

beforeAll(async () => {
  browser = await chromium.launch({
    executablePath: process.env.MOEBIUS_E2E_CHROMIUM || undefined,
  });
  server = await startServer();
});

afterAll(async () => {
  await browser.close();
  await server.stop().catch(() => {});
});

/** A page of its own, signed in, in one theme at the width the standard assumes. */
async function open(theme: "dark" | "light", path: string): Promise<Page> {
  const context = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    colorScheme: theme,
  });
  const page = await context.newPage();
  const answer = await page.request.post(`${server.url}/v1/session`, {
    headers: { Origin: server.url, "X-Moebius": "1" },
    data: { token: server.token },
  });
  expect(answer.status()).toBe(204);
  await page.goto(server.url);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  await page.goto(`${server.url}${path}`);
  await page.waitForLoadState("networkidle").catch(() => {});
  return page;
}

/** Adds one element to the page, with the style of a made-up pill or a made-up bystander. */
async function add(page: Page, style: string, text: string) {
  await page.evaluate(
    ({ style, text }) => {
      const el = document.createElement("span");
      el.setAttribute("style", `display:inline-block;${style}`);
      el.textContent = text;
      document.body.appendChild(el);
    },
    { style, text },
  );
}

describe("state pills", () => {
  for (const theme of ["dark", "light"] as const) {
    it(`are found when built any way in ${theme}`, async () => {
      const page = await open(theme, "/inbox");
      expect(await page.evaluate(findPills)).toEqual([]);
      const fill = "var(--error-fill)";
      await add(page, `background:${fill};border-radius:9999px;padding:2px 8px`, "Failed");
      await add(
        page,
        "background:color-mix(in srgb,var(--attention-fill) 100%,transparent);border-radius:6px;padding:2px 8px;height:24px",
        "Waiting",
      );
      await add(
        page,
        "background:var(--complete-fill);border-radius:6px;padding:2px 8px;height:20px",
        "Done",
      );
      await add(
        page,
        "border:1px solid var(--progress-text);border-radius:6px;padding:2px 8px;height:22px",
        "Running",
      );
      expect((await page.evaluate(findPills)).length).toBe(4);
      await page.context().close();
    });

    it(`leaves a banner, a neutral chip, a tall tinted button and a red delete button alone in ${theme}`, async () => {
      const page = await open(theme, "/inbox");
      await add(
        page,
        "background:var(--error-fill);border-radius:8px;padding:12px;width:600px;height:48px",
        "The files of this agent cannot be read",
      );
      await add(
        page,
        "background:var(--selected);border-radius:6px;padding:2px 8px;height:28px",
        "All",
      );
      await add(
        page,
        "background:var(--attention-fill);border-radius:8px;padding:8px 16px;height:40px",
        "Try again",
      );
      await add(page, "color:var(--error-text)", "Failed");
      await page.evaluate(() => {
        const button = document.createElement("button");
        button.setAttribute(
          "style",
          "background:var(--error-fill);border-radius:6px;padding:2px 8px;height:28px",
        );
        button.textContent = "Delete";
        document.body.appendChild(button);
      });
      expect(await page.evaluate(findPills)).toEqual([]);
      await page.context().close();
    });
  }

  for (const theme of ["dark", "light"] as const) {
    it(`are not on the pages that carried them in ${theme}`, async () => {
      for (const path of [
        "/inbox",
        "/fleet",
        "/board/1",
        "/runs/1000",
        "/chat",
        "/retro",
        "/experiments",
        "/rules",
        "/workflows",
        "/automations",
        "/plugins",
        "/agents",
      ]) {
        const page = await open(theme, path);
        expect(await page.evaluate(findPills), `${theme} ${path}`).toEqual([]);
        await page.context().close();
      }
    });
  }
});
