/**
 * Kinds of issues in a real browser: an owner defines a kind with a choice and a number field and lets two projects pick
 * it, and the Issues page, which shows the cards of both projects, is filtered by the choice and ordered by the number.
 * Every wait is on something the page shows. The cards are the fake server's own (e2e/fake/kinds.ts).
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

/** The titles of the issues the Issues page lists, in order. */
async function titles(page: Page): Promise<string[]> {
  // By test id rather than by role: while a menu is open or closing the page behind it is hidden from the roles.
  const links = page.locator('[data-testid^="issue-"] a');
  return (await links.allTextContents()).map((t) => t.trim());
}

describe.skipIf(usesRealServer)("kinds of issues", () => {
  it("defines a kind for two projects and filters and sorts the issues across them by its fields", async () => {
    server = await startServer();
    const context = await browser.newContext();
    const page = await context.newPage();
    page.setDefaultTimeout(8000);
    await page.goto(server.loginUrl);
    await page.getByRole("navigation", { name: "Pages" }).waitFor();
    // A second project, made as the signed-in owner (the cookie, and the header the server wants with it).
    const made = await context.request.post(`${server.url}/v1/projects`, {
      headers: { "x-moebius": "1" },
      data: { name: "Storefront", key: "SHOP" },
    });
    expect(made.status()).toBe(200);

    // The Kinds page: a kind with a choice and a number, picked by both projects.
    await page.goto(`${server.url}/kinds`);
    await page.getByTestId("kinds-page").waitFor();
    await page.getByLabel("Name", { exact: true }).fill("Bug");
    await page.getByRole("button", { name: "Add a field" }).click();
    await page.getByLabel("Name of field 1").fill("Severity");
    await page.getByRole("button", { name: "Type of field 1" }).click();
    await page.getByRole("menuitemradio", { name: "Choice" }).click();
    await page.getByLabel("Options of field 1").fill("low, high");
    await page.getByRole("button", { name: "Add a field" }).click();
    await page.getByLabel("Name of field 2").fill("Estimate");
    await page.getByRole("button", { name: "Type of field 2" }).click();
    await page.getByRole("menuitemradio", { name: "Number" }).click();
    await page.getByRole("button", { name: "Add kind" }).click();
    const bug = page.getByTestId("kind-Bug");
    await bug.waitFor();
    for (const project of ["demo", "Storefront"]) {
      // A box is controlled, so it flips when the server has answered; the click waits for it to be enabled again.
      await bug.getByLabel(project, { exact: true }).click();
    }
    await bug.getByRole("button", { name: "Delete the kind Bug" }).waitFor();
    await page.waitForFunction(
      () =>
        document.querySelectorAll('[data-testid="kind-Bug"] input[type=checkbox]:checked')
          .length === 2,
    );

    // The Issues page lists the cards of both projects.
    await page.goto(`${server.url}/issues`);
    await page.getByTestId("issues-page").waitFor();
    await page.getByTestId("issue-3").waitFor();
    expect(await titles(page)).toEqual([
      "Login is broken",
      "A typo on the home page",
      "Checkout fails",
    ]);

    // Filtered by a field of the kind, the cards of both projects that have the value are left.
    const issues = page.getByTestId("issues-page");
    await issues.getByRole("button", { name: "Filter", exact: true }).click();
    await page.getByRole("menuitem", { name: "Severity" }).click();
    await page.getByRole("menuitemradio", { name: "high" }).click();
    await issues.getByRole("button", { name: "Severity is high", exact: true }).waitFor();
    await page.waitForFunction(
      () => document.querySelectorAll('[data-testid^="issue-"]').length === 2,
    );
    expect(await titles(page)).toEqual(["Login is broken", "Checkout fails"]);
    expect(new URL(page.url()).searchParams.get("field.Severity")).toBe("high");

    // And ordered by another one.
    await issues.getByRole("button", { name: "Display" }).click();
    await page.getByRole("menuitemradio", { name: "Estimate, lowest first" }).click();
    await page.keyboard.press("Escape");
    await page.waitForFunction(
      () => document.querySelector('[data-testid^="issue-"] a')?.textContent === "Checkout fails",
    );
    expect(await titles(page)).toEqual(["Checkout fails", "Login is broken"]);
    if (!server.kinds) throw new Error("this test needs the fake server");
    const asked = server.kinds.calls.filter((c) => c.path.startsWith("/v1/issues?")).at(-1);
    expect(asked?.path).toBe("/v1/issues?field.Severity=high&sort=Estimate");

    // A row opens its card on its board, and Back returns to the Issues with the filter and the order still in the address.
    await page.getByRole("link", { name: "Login is broken" }).click();
    await page.waitForURL("**/board/1?card=1");
    await page.getByRole("region", { name: "Card inspector" }).waitFor();
    await page.goBack();
    await page.getByTestId("issues-page").waitFor();
    expect(new URL(page.url()).searchParams.get("field.Severity")).toBe("high");
    expect(new URL(page.url()).searchParams.get("sort")).toBe("asc:Estimate");
    await context.close();
  });
});
