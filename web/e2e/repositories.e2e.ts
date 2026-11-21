/**
 * A card over several repositories in a real browser: a member adds a second repository to a card from its properties
 * and sees it listed, the server's reason is shown when a repository with an open change is taken off, and a card with
 * changes in two repositories shows both, each with its repository and checks, in the window of its properties. The cards
 * are the fake server's own (e2e/fake/board.ts), which has no counterpart against a real server.
 */

import { type Browser, type BrowserContext, chromium, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { unfold } from "./controls";
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

const card = (number: number) => page.getByTestId(`card-${number}`);
const properties = (number: number) =>
  page.getByRole("region", { name: `Properties of #${number}`, exact: true });

/** Opens the card's window over the board, then the window of its properties from the card's menu. */
async function openProperties(number: number) {
  await card(number).click();
  await page
    .locator("[data-card-window]")
    .getByRole("button", { name: `More actions for #${number}`, exact: true })
    .click();
  await page.getByRole("menuitem", { name: "Properties…" }).click();
  await properties(number).waitFor();
}

/** A server whose project has a second repository, `web`, and the board of `demo` open. */
async function openBoard() {
  server = await startServer();
  context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  const headers = { "X-Moebius": "1" };
  const made = await context.request.post(`${server.url}/v1/repositories`, {
    headers,
    data: { name: "web", url: "/srv/web" },
  });
  expect(made.ok(), await made.text()).toBe(true);
  const { id } = (await made.json()) as { id: number };
  const linked = await context.request.post(`${server.url}/v1/projects/1/repositories`, {
    headers,
    data: { repository: id },
  });
  expect(linked.ok(), await linked.text()).toBe(true);
  await page.goto(`${server.url}/board/1`);
  await page.getByRole("heading", { name: "demo", level: 1 }).waitFor();
}

describe.skipIf(usesRealServer)("the repositories of a card", () => {
  it("adds a second repository from the card's properties and lists it", async () => {
    await openBoard();
    await openProperties(3);
    const repositories = properties(3).getByRole("button", { name: /^Repositories/ });
    await repositories.waitFor();
    expect(await repositories.textContent()).toContain("demo (own)");
    await repositories.click();
    await page.getByRole("menuitemcheckbox", { name: "web" }).click();
    await page.getByRole("button", { name: /^Repositories.*web/ }).waitFor();
    expect(server.board?.requests.at(-1)).toMatchObject({
      method: "PUT",
      path: "/v1/cards/3/repositories",
      body: { repositories: ["web"] },
    });
  });

  it("says why a repository with an open change cannot be taken off, and keeps it listed", async () => {
    await openBoard();
    server.board?.seedChange(5);
    await openProperties(5);
    await properties(5)
      .getByRole("button", { name: /^Repositories.*web/ })
      .click();
    await page.getByRole("menuitemcheckbox", { name: "web" }).click();
    await page
      .getByRole("alert")
      .filter({ hasText: "the card has an open change in web" })
      .waitFor();
    await properties(5)
      .getByRole("button", { name: /^Repositories.*web/ })
      .waitFor();
  });

  it("shows each change of a card with its repository and checks, in the window of its properties", async () => {
    await openBoard();
    server.board?.seedChange(5);
    await openProperties(5);
    const changes = page.getByTestId("card-change");
    await changes.nth(1).waitFor();
    expect(await changes.count()).toBe(2);
    expect(await changes.nth(0).textContent()).toContain("demo");
    expect(await changes.nth(0).textContent()).toContain("Checks: Green");
    expect(await changes.nth(1).textContent()).toContain("web");
    expect(await changes.nth(1).textContent()).toContain("Checks: Failed");
    const facts = properties(5);
    await unfold(facts);
    await facts.getByText("Checks in demo").waitFor();
    await facts.getByText("Checks in web").waitFor();
  });
});
