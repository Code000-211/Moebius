/**
 * The software factory in a real browser (ADR 0032), against the fake server: the owner's daily digest is an item of the
 * Inbox that tells what happened per repository and what needs a person, and is dismissed when read; a repository's
 * "Stale handling" box saves its days; a project's Backlog (in its Repositories settings) saves where the file is and
 * previews and runs a sync. Set MOEBIUS_SHOT_DIR to also write the pictures of the digest and of the two settings
 * (light and dark, 1280 px wide, twice the pixels).
 */
import { mkdirSync } from "node:fs";
import { join } from "node:path";
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

const SHOTS = process.env.MOEBIUS_SHOT_DIR;

async function open(path: string, scheme: "light" | "dark" = "light") {
  server = await startServer();
  if (!server.board || !server.factory) throw new Error("this test needs the fake server");
  const digest = server.board.seedDigest();
  server.factory.seedBacklog();
  context = await browser.newContext({
    viewport: { width: 1280, height: 900 },
    colorScheme: scheme,
    deviceScaleFactor: 2,
  });
  page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  await page.goto(`${server.url}${path}`);
  return digest;
}

async function shoot(name: string) {
  if (!SHOTS) return;
  mkdirSync(SHOTS, { recursive: true });
  await page.screenshot({ path: join(SHOTS, `${name}.png`) });
}

const digestRow = () => page.locator('[data-testid="inbox-item"][data-kind="daily_digest"]');

describe.skipIf(usesRealServer)("the software factory", () => {
  it("shows the digest in the Inbox with what happened and what needs a person, and dismisses it", async () => {
    await open("/inbox");
    await digestRow().waitFor();
    expect(await digestRow().textContent()).toContain("Daily digest 2026-11-25");
    expect(await digestRow().textContent()).not.toContain("chat");
    await digestRow().click();
    const shown = page.getByTestId("digest");
    await shown.waitFor();
    const demo = shown.getByRole("region", { name: "demo" });
    expect(await demo.textContent()).toContain("Cache the compiled plan");
    expect(await demo.textContent()).toContain("measured");
    expect(await demo.textContent()).toContain("the checks of pull request 31 failed");
    expect(await demo.textContent()).toContain("round 2: approved");
    expect(await demo.textContent()).toContain("released: taken 15 days ago");
    expect(await shown.getByTestId("digest-tokens").textContent()).toContain(
      "412,300 since the last digest, against a budget of 1,000,000 a day",
    );
    expect(await shown.getByTestId("digest-needs").textContent()).toContain("Needs a person (2)");
    // A line of what needs a person opens its card.
    await shown.getByRole("button", { name: /#5 Speed up the build/ }).click();
    await page.waitForURL("**card=5**");
    await page.keyboard.press("Escape");
  });

  it("dismisses the digest", async () => {
    const id = await open("/inbox");
    await digestRow().click();
    await page.getByTestId("digest").waitFor();
    // The window scrolls: the last block and the button under it can be brought into the view of a 900 px page.
    const dismiss = page.getByRole("button", { name: "Dismiss" });
    await dismiss.scrollIntoViewIfNeeded();
    const box = await dismiss.boundingBox();
    expect(box && box.y >= 0 && box.y + box.height <= 900).toBe(true);
    await dismiss.click();
    await page.waitForFunction(
      () =>
        document.querySelectorAll('[data-testid="inbox-item"][data-kind="daily_digest"]').length ===
        0,
    );
    const requests = server.board?.requests ?? [];
    expect(requests.some((r) => r.method === "POST" && r.path === `/v1/digest/${id}/dismiss`)).toBe(
      true,
    );
  });

  it("saves the days of a repository's stale handling", async () => {
    await open("/projects/1/repositories/1");
    const box = page.getByTestId("stale-handling");
    await box.getByLabel("Release an issue taken for (days)").waitFor();
    expect(await box.getByLabel("Close after (days)").inputValue()).toBe("30");
    await box.getByLabel("Release an issue taken for (days)").fill("7");
    await box.getByLabel("Close after (days)").fill("45");
    await box.getByRole("button", { name: "Save settings" }).click();
    await page.waitForFunction(
      () =>
        (
          document.querySelector(
            '[data-testid="stale-handling"] button[type="submit"]',
          ) as HTMLButtonElement
        )?.disabled === true,
    );
    const put = server.factory?.requests.findLast(
      (r) => r.method === "PUT" && r.path === "/v1/repositories/1/upkeep",
    );
    expect(put?.body).toEqual({
      release_after_days: 7,
      stale_after_days: 14,
      close_after_days: 45,
    });
  });

  it("saves a project's backlog and previews and runs its sync", async () => {
    await open("/projects/1/settings/repositories");
    const settings = page.getByTestId("backlog-settings");
    await settings.getByLabel("File").waitFor();
    expect(await settings.getByLabel("File").inputValue()).toBe(
      "notes/backlog/optimisation-backlog.md",
    );
    await settings.getByLabel("Most issues one sync makes").fill("10");
    await settings.getByRole("button", { name: "Save backlog" }).click();
    await settings.getByRole("button", { name: "Sync now" }).waitFor();
    await page.waitForFunction(
      () =>
        (
          document.querySelector(
            '[data-testid="backlog-settings"] button[type="submit"]',
          ) as HTMLButtonElement | null
        )?.disabled === true,
    );
    const put = server.factory?.requests.findLast(
      (r) => r.method === "PUT" && r.path === "/v1/projects/1/backlog",
    );
    expect(put?.body).toMatchObject({ cap: 10 });
    await settings.getByRole("button", { name: "Preview a sync" }).click();
    const report = settings.getByTestId("sync-report");
    await report.waitFor();
    expect(await report.textContent()).toContain("A sync would do this");
    expect(await report.textContent()).toContain("K-47: Cache the compiled plan");
    await settings.getByRole("button", { name: "Sync now" }).click();
    await page.waitForFunction(() =>
      document.querySelector('[data-testid="sync-report"]')?.textContent?.startsWith("Synced"),
    );
    expect(await settings.getByTestId("sync-report").textContent()).toContain("#41 K-47");
  });

  for (const scheme of ["light", "dark"] as const) {
    it(`draws the digest and the settings (${scheme})`, async () => {
      await open("/inbox", scheme);
      await digestRow().click();
      await page.getByTestId("digest").waitFor();
      await page
        .getByTestId("inbox-decision")
        .evaluate((n) => n.scrollIntoView({ block: "start" }));
      await shoot(`digest-inbox-${scheme}`);
      await page.goto(`${server.url}/projects/1/repositories/1`);
      await page.getByTestId("stale-handling").getByLabel("Remind after (days)").waitFor();
      await page.getByTestId("stale-handling").scrollIntoViewIfNeeded();
      await shoot(`settings-stale-handling-${scheme}`);
      await page.goto(`${server.url}/projects/1/settings/repositories`);
      await page.getByTestId("backlog-settings").getByLabel("File").waitFor();
      await page.getByRole("button", { name: "Preview a sync" }).click();
      await page.getByTestId("sync-report").waitFor();
      await shoot(`settings-backlog-${scheme}`);
    });
  }
});
