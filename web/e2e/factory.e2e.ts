/**
 * The first part of the software factory, in a real browser against the fake server: the "Working with others" box holds
 * the merging settings (required measurement, locked paths, daily limit, review brief, ready-for-testing label,
 * measurers) and saves them; a person freezes and unfreezes the repository, and a frozen repository says "Frozen: <reason>"
 * in the card window; a followed pull request whose code the Reviewer approved while its measurement is missing says so.
 * Set MOEBIUS_SHOT_DIR to also write the pictures (light and dark, 1280 px wide, twice the pixels).
 */
import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { type Browser, type BrowserContext, chromium, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import type { FakeServer } from "./fakeServer";
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

const SHOTS = process.env.MOEBIUS_SHOT_DIR;

async function open(scheme: "light" | "dark" = "light") {
  server = await startServer();
  if (!server.board) throw new Error("this test needs the fake server");
  const approved = server.board.seedFactory();
  context = await browser.newContext({
    viewport: { width: 1280, height: 900 },
    colorScheme: scheme,
    deviceScaleFactor: 2,
  });
  page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  return approved;
}

async function settings() {
  await page.goto(`${server.url}/projects/1/repositories/1`);
  const box = page.getByTestId("working-with-others");
  await box.waitFor();
  return box;
}

async function shoot(name: string, scheme: "light" | "dark", target?: ReturnType<Page["locator"]>) {
  if (!SHOTS) return;
  mkdirSync(SHOTS, { recursive: true });
  const path = join(SHOTS, `${name}-${scheme}.png`);
  if (target) await target.screenshot({ path });
  else await page.screenshot({ path });
}

describe.skipIf(usesRealServer)("the factory", () => {
  it("saves the merging settings of a repository", async () => {
    await open();
    const box = await settings();
    await box.getByLabel("Require a measurement").click();
    await box.getByLabel("Locked paths").fill("kernels/**\ndocs/legal/**");
    await box.getByLabel("Merges a day").fill("6");
    await box.getByLabel("Review brief").fill("notes/backlog/CARDS.md\nAGENTS.md");
    await box.getByLabel("Ready for testing").fill("needs-bench");
    await box.getByLabel("Measurers").fill("alice\nbob");
    await box.getByRole("button", { name: "Save settings" }).click();
    await page.waitForFunction(
      () =>
        (
          document.querySelector('[data-testid="working-with-others"] button[type="submit"]') as
            | HTMLButtonElement
            | undefined
        )?.disabled === true,
    );
    const put = (server as FakeServer).pagesA.requests.find(
      (r) => r.method === "PUT" && r.path === "/v1/repositories/1",
    );
    expect(put?.body).toMatchObject({
      require_measurement: true,
      locked_paths: ["kernels/**", "docs/legal/**"],
      daily_merge_limit: 6,
      review_brief: ["notes/backlog/CARDS.md", "AGENTS.md"],
      testing_label: "needs-bench",
      measurers: ["alice", "bob"],
    });
  });

  it("freezes a repository, says so in the card window, and unfreezes it", async () => {
    await open();
    const box = await settings();
    await box.getByRole("button", { name: "Freeze this repository" }).click();
    await box.getByTestId("frozen-notice").waitFor();
    expect(await box.getByTestId("frozen-notice").textContent()).toContain("Frozen:");
    await page.goto(`${server.url}/projects/1`);
    await page.getByTestId("card-1").click();
    const window = page.locator("[data-card-window]");
    await window.getByTestId("frozen-notice").waitFor();
    expect(await window.getByTestId("frozen-notice").textContent()).toContain("Frozen: ");
    await page.keyboard.press("Escape");
    const again = await settings();
    await again.getByRole("button", { name: "Unfreeze" }).click();
    await again.getByRole("button", { name: "Freeze this repository" }).waitFor();
    expect(await again.getByTestId("frozen-notice").count()).toBe(0);
  });

  it("shows a followed pull request whose code is approved and whose measurement is missing", async () => {
    const approved = await open();
    await page.goto(`${server.url}/projects/1`);
    await page.getByTestId(`card-${approved}`).click();
    const review = page.locator("[data-card-window]").getByTestId("external-review");
    await review.waitFor();
    expect(await review.textContent()).toContain("Code approved, measurement needed");
    expect(await review.textContent()).toContain("## Measurement");
  });

  for (const scheme of ["light", "dark"] as const) {
    it(`draws the pictures (${scheme})`, async () => {
      if (!SHOTS) return;
      {
        const approved = await open(scheme);
        const box = await settings();
        await box.getByLabel("Measurers").fill("alice\nbob");
        await box.getByLabel("Locked paths").fill("kernels/**");
        // The box scrolls inside the page, so the picture is taken with a window as tall as the box.
        await page.setViewportSize({ width: 1280, height: 3000 });
        await box.scrollIntoViewIfNeeded();
        await shoot("settings", scheme, box);
        await page.setViewportSize({ width: 1280, height: 900 });
        await page.goto(`${server.url}/projects/1`);
        await page.getByTestId(`card-${approved}`).click();
        await page.locator("[data-card-window]").getByTestId("external-review").waitFor();
        await shoot("approve-code", scheme);
        await page.keyboard.press("Escape");
        const again = await settings();
        await again.getByRole("button", { name: "Freeze this repository" }).click();
        await again.getByTestId("frozen-notice").waitFor();
        await page.goto(`${server.url}/projects/1`);
        await page.getByTestId(`card-${approved}`).click();
        await page.locator("[data-card-window]").getByTestId("frozen-notice").waitFor();
        await shoot("frozen", scheme);
      }
    });
  }
});
