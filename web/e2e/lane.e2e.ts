/**
 * The test lane (ADR 0035) in a real browser against the fake server: the repository's "Test lane" box chooses a lane and
 * warns, in words, while the lane is not isolated everywhere; a lane that paused itself is put back to work; and the card
 * window shows the lane's report: a measurement with its rows, a failure with its output, a job that runs.
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
  context = await browser.newContext({
    viewport: { width: 1280, height: 900 },
    colorScheme: scheme,
    deviceScaleFactor: 2,
  });
  page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  return (server as FakeServer).lane;
}

async function box() {
  await page.goto(`${server.url}/projects/1/repositories/1`);
  const found = page.getByTestId("test-lane");
  await found.waitFor();
  return found;
}

async function shoot(name: string, scheme: "light" | "dark", target?: ReturnType<Page["locator"]>) {
  if (!SHOTS) return;
  mkdirSync(SHOTS, { recursive: true });
  const path = join(SHOTS, `${name}-${scheme}.png`);
  if (target) await target.screenshot({ path });
  else await page.screenshot({ path });
}

/** The card window of the card the board's factory seed adds: a followed pull request the Reviewer approved. */
async function cardWindow(card: number) {
  await page.goto(`${server.url}/projects/1`);
  await page.getByTestId(`card-${card}`).click();
  const window = page.locator("[data-card-window]");
  await window.getByTestId("lane-report").waitFor();
  return window;
}

describe.skipIf(usesRealServer)("the test lane", () => {
  it("chooses a lane for the repository and shows its machine without a warning while it is isolated", async () => {
    const lane = await open();
    lane.seed({ assigned: false });
    const found = await box();
    expect(await found.getByTestId("lane-machine").count()).toBe(0);
    await found.getByRole("button", { name: "Lane" }).click();
    await page.getByRole("menuitemradio", { name: "gpu1" }).click();
    await found.getByTestId("lane-machine").waitFor();
    const machine = (await found.getByTestId("lane-machine").textContent()) ?? "";
    expect(machine).toContain("2x MI300X");
    expect(machine).toContain("rocm/dev-ubuntu-24.04:6.2");
    expect(await found.getByTestId("isolation-warning").count()).toBe(0);
    expect(lane.requests.at(-1)?.body).toEqual({ lane: "gpu1" });
    // Choosing none takes the lane away again.
    await found.getByRole("button", { name: "Lane" }).click();
    await page.getByRole("menuitemradio", { name: "No lane" }).click();
    await page.waitForFunction(
      () => !document.querySelector('[data-testid="test-lane"] [data-testid="lane-machine"]'),
    );
    expect(lane.requests.at(-1)?.body).toEqual({ lane: null });
  });

  it("warns, in words, while the lane is not isolated everywhere", async () => {
    const lane = await open();
    lane.seed({ isolation: "off" });
    const found = await box();
    const warning = found.getByTestId("isolation-warning");
    await warning.waitFor();
    expect(await warning.textContent()).toContain("Isolation is off");
    expect(await warning.textContent()).toContain("directly on the machine");
    expect(await found.getByTestId("lane-machine").textContent()).toContain(
      "Every job runs directly on the machine",
    );
  });

  it("puts a lane that paused itself back to work", async () => {
    const lane = await open();
    lane.seed({ paused: true });
    const found = await box();
    const paused = found.getByTestId("lane-paused");
    await paused.waitFor();
    expect(await paused.textContent()).toContain("3 jobs in a row");
    await found.getByRole("button", { name: "Resume the lane" }).click();
    await page.waitForFunction(
      () => !document.querySelector('[data-testid="test-lane"] [data-testid="lane-paused"]'),
    );
    expect(lane.requests.some((r) => r.path === "/v1/lanes/gpu1/resume")).toBe(true);
  });

  it("shows the report of a measurement in the card window", async () => {
    const lane = await open();
    lane.seed();
    const approved = (server as FakeServer).board.seedFactory();
    lane.seedCard(approved, "measured");
    const window = await cardWindow(approved);
    expect(await window.getByTestId("lane-state").textContent()).toBe("Measured");
    const table = (await window.getByTestId("lane-measurement").textContent()) ?? "";
    expect(table).toContain("decode_tps (tok/s)");
    expect(table).toContain("40.6");
    expect(table).toContain("44.5");
    expect(table).toContain("+9.61%");
    expect(table).toContain("Output parity: identical");
    expect(await window.getByTestId("lane-job-machine").textContent()).toContain(
      "in a container (rocm/dev-ubuntu-24.04:6.2), no network",
    );
  });

  it("shows the step a failure stopped at and a job that is running", async () => {
    const lane = await open();
    lane.seed();
    const failing = (server as FakeServer).board.seedFactory();
    lane.seedCard(failing, "failed");
    let window = await cardWindow(failing);
    expect(await window.getByTestId("lane-failure").textContent()).toContain(
      "Stopped at gpu selftests: it exited with code 3",
    );
    expect(await window.getByTestId("lane-failure").textContent()).toContain("max abs error");
    await page.keyboard.press("Escape");
    const running = (server as FakeServer).board.seedFactory();
    lane.seedCard(running, "running");
    window = await cardWindow(running);
    expect(await window.getByTestId("lane-job").textContent()).toContain("Running");
    expect(await window.getByTestId("lane-job").textContent()).toContain("run 2 of 3");
  });

  for (const scheme of ["light", "dark"] as const) {
    it(`draws the pictures (${scheme})`, async () => {
      if (!SHOTS) return;
      const lane = await open(scheme);
      lane.seed({ isolation: "off" });
      const approved = (server as FakeServer).board.seedFactory();
      lane.seedCard(approved, "measured");
      // The settings of a lane that is not isolated everywhere, with its warning.
      await page.setViewportSize({ width: 1280, height: 3000 });
      const found = await box();
      await found.getByTestId("isolation-warning").waitFor();
      await found.scrollIntoViewIfNeeded();
      await shoot("lane-settings-off", scheme, found);
      // The same box for a lane that isolates every job.
      lane.seed({ isolation: "always" });
      await page.reload();
      const isolated = page.getByTestId("test-lane");
      await isolated.getByTestId("lane-machine").waitFor();
      await shoot("lane-settings-always", scheme, isolated);
      // A card with the report of its measurement.
      await page.setViewportSize({ width: 1280, height: 900 });
      await cardWindow(approved);
      await shoot("lane-card-report", scheme);
    });
  }
});

// A page of the box is its own shape in a narrow window: the table and the warning wrap instead of widening the page.
describe.skipIf(usesRealServer)("the test lane in a narrow window", () => {
  it("does not widen the page", async () => {
    const lane = await open();
    lane.seed({ isolation: "external_only" });
    await page.setViewportSize({ width: 390, height: 800 });
    const found = await box();
    await found.getByTestId("isolation-warning").waitFor();
    const width = await page.evaluate(() => document.documentElement.scrollWidth);
    expect(width).toBeLessThanOrEqual(390);
  });
});
