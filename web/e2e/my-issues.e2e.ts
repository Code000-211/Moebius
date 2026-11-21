/**
 * My issues in a real browser: the page lists only the issues given to the signed-in person, grouped by status, and a
 * row opens the card window over the page. The cards are the fake server's own (e2e/fake/kinds.ts): two of them are
 * the owner's, one is Tom's.
 */
import { type Browser, chromium } from "playwright";
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

describe.skipIf(usesRealServer)("my issues", () => {
  it("lists only the issues of the person who is signed in and opens a row as a card", async () => {
    server = await startServer();
    const page = await (await browser.newContext()).newPage();
    page.setDefaultTimeout(8000);
    await page.goto(server.loginUrl);
    await page.getByRole("navigation", { name: "Pages" }).waitFor();
    await page.goto(`${server.url}/my-issues`);

    const list = page.getByRole("list", { name: "My issues" });
    await list.waitFor();
    const rows = page.getByTestId("list-row");
    await rows.first().waitFor();
    const titles = (await rows.allTextContents()).join("|");
    expect(titles).toContain("Login is broken");
    expect(titles).toContain("Checkout fails");
    expect(titles).not.toContain("A typo on the home page");
    expect(await page.getByTestId("group-row").allTextContents()).toEqual(["Not started"]);

    await page.getByText("Login is broken").click();
    await page.waitForURL(/\/my-issues\?card=1$/);
  });
});
