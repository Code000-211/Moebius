/**
 * The More page in a real browser: it lists the pages with no other home and a row leads to its page (Usage).
 */
import { type Browser, chromium } from "playwright";
import { afterAll, beforeAll, expect, it } from "vitest";
import { startServer } from "./server";

let browser: Browser;

beforeAll(async () => {
  browser = await chromium.launch({
    executablePath: process.env.MOEBIUS_E2E_CHROMIUM || undefined,
  });
});

afterAll(async () => {
  await browser.close();
});

it("opens More and reaches Usage", async () => {
  const server = await startServer();
  const context = await browser.newContext({ viewport: { width: 1280, height: 720 } });
  try {
    const page = await context.newPage();
    await page.goto(server.loginUrl);
    await page.getByRole("navigation", { name: "Pages" }).waitFor();
    await page.goto(new URL("/more", page.url()).toString());
    const list = page.getByRole("navigation", { name: "More pages" });
    await list.getByRole("link", { name: "Usage" }).waitFor();
    expect(await list.getByRole("link").count()).toBe(13);
    await list.getByRole("link", { name: "Usage" }).click();
    await page.waitForURL(/\/usage$/);
    expect(page.url()).toMatch(/\/usage$/);
  } finally {
    await context.close();
    await server.stop().catch(() => {});
  }
});
