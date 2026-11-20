/**
 * Choosing several cards or runs at once, in a real browser: the box on a card chosen with the keyboard and with the
 * mouse (a click and a shift-click for a range), the bar at the foot that says how many are chosen and moves, blocks or
 * stops them in one go, the right-click menu with the same commands and their keys, Escape letting go, and Fleet
 * stopping two runs together. The fake server has the data, so these tests are skipped against a real server.
 */
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

async function fresh() {
  server = await startServer();
  context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
}

afterEach(async () => {
  await context.close();
  await server.stop().catch(() => {});
});

const fake = () => {
  if (!server.board) throw new Error("this test needs the fake server");
  return server.board;
};
const box = (name: string | RegExp) => page.getByRole("checkbox", { name });
const bar = () => page.getByRole("toolbar", { name: "Chosen" });
const says = (text: string) => bar().getByRole("status").filter({ hasText: text });

/** Opens the board with two more outside issues that wait for approval, so three cards can be approved together. */
async function openBoard(): Promise<number[]> {
  const more = fake().seedApprovals(2);
  await page.goto(`${server.url}/board/1`);
  await page.getByRole("heading", { name: "demo", level: 1 }).waitFor();
  await page.getByTestId(`card-${more[1]}`).waitFor();
  return [2, ...more];
}

const approved = () =>
  fake()
    .requests.filter((r) => /\/v1\/cards\/\d+\/approve$/.test(r.path))
    .map((r) => r.path)
    .sort();

describe.skipIf(usesRealServer)("choosing cards on the board", () => {
  it("chooses three cards with the keyboard and moves them in one go", async () => {
    await fresh();
    const [a, b, c] = await openBoard();
    for (const number of [a, b, c]) {
      await box(`Choose #${number}`).focus();
      await page.keyboard.press("Space");
    }
    await says("3 cards chosen").waitFor();
    // The bar offers the move that a drop in the column would make, and says how many cards it reaches.
    await bar().getByRole("button", { name: "Move" }).click();
    await page.getByRole("menuitem", { name: /Move to Not started/ }).click();
    await page
      .getByRole("status")
      .filter({ hasText: `Moved to Not started: #${a}` })
      .waitFor();
    expect(approved()).toEqual([a, b, c].map((n) => `/v1/cards/${n}/approve`).sort());
    // The cards are let go once they have moved.
    await bar().waitFor({ state: "detached" });
  });

  it("chooses three cards with the mouse, a click and a shift-click, and moves them with a key", async () => {
    await fresh();
    const [a, b, c] = await openBoard();
    await page.getByTestId(`card-${a}`).hover();
    await box(`Choose #${a}`).click();
    await box(`Choose #${c}`).click({ modifiers: ["Shift"] });
    await says("3 cards chosen").waitFor();
    expect(await box(`Choose #${b}`).isChecked()).toBe(true);
    // The key of the command, which the move menu of the bar also shows.
    await bar().getByRole("button", { name: "Move" }).click();
    await page
      .getByRole("menuitem", { name: /Move to Not started/ })
      .getByText("N", { exact: true })
      .waitFor();
    await page.keyboard.press("Escape");
    await says("3 cards chosen").waitFor();
    await box(`Choose #${a}`).focus();
    await page.keyboard.press("n");
    await page.getByRole("status").filter({ hasText: "Moved to Not started" }).waitFor();
    expect(approved()).toHaveLength(3);
  });

  it("opens the same commands with a right click, and Escape clears the choice", async () => {
    await fresh();
    const [a, b] = await openBoard();
    await box(`Choose #${a}`).focus();
    await page.keyboard.press("Space");
    await box(`Choose #${b}`).focus();
    await page.keyboard.press("Space");
    await says("2 cards chosen").waitFor();
    // A right click on a chosen card keeps the choice and lists its commands with their keys.
    await page.getByTestId(`card-${b}`).click({ button: "right" });
    const menu = page.getByRole("menu", { name: "2 cards" });
    await menu.getByRole("menuitem", { name: /^Assign to me/ }).waitFor();
    await menu.getByRole("menuitem", { name: /Block/ }).waitFor();
    await page.keyboard.press("Escape");
    await menu.waitFor({ state: "detached" });
    // The first Escape closed the menu only; the choice stands until the next one.
    await says("2 cards chosen").waitFor();
    await page.keyboard.press("Escape");
    await bar().waitFor({ state: "detached" });
    expect(await box(`Choose #${a}`).isChecked()).toBe(false);
  });

  it("blocks several cards with one reason", async () => {
    await fresh();
    const [a, b] = await openBoard();
    await box(`Choose #${a}`).focus();
    await page.keyboard.press("Space");
    await box(`Choose #${b}`).focus();
    await page.keyboard.press("Space");
    await bar()
      .getByRole("button", { name: /^Block/ })
      .click();
    const dialog = page.getByRole("dialog", { name: "Block 2 cards" });
    await dialog.getByRole("textbox").fill("Wait for the release");
    await dialog.getByRole("button", { name: "Block", exact: true }).click();
    await page.getByRole("status").filter({ hasText: "Blocked 2 cards" }).waitFor();
    const blocks = fake().requests.filter((r) => /\/block$/.test(r.path));
    expect(blocks).toHaveLength(2);
    expect(
      blocks.every((r) => (r.body as { reason: string }).reason === "Wait for the release"),
    ).toBe(true);
  });
});

describe.skipIf(usesRealServer)("choosing runs in Fleet", () => {
  it("stops two runs together", async () => {
    await fresh();
    // Two chats that ended take the first ids, as the board's run ids do; the two that are going come after them.
    server.chat?.seed({ status: "succeeded", title: "Old one" });
    server.chat?.seed({ status: "succeeded", title: "Old two" });
    const first = server.chat?.seed({ status: "running", title: "Write the first rule" }) as number;
    const second = server.chat?.seed({
      status: "running",
      title: "Write the second rule",
    }) as number;
    await page.goto(`${server.url}/fleet`);
    await expect.poll(() => page.getByTestId("fleet-row").count()).toBe(3);
    await box("Choose Write the first rule").check();
    await box("Choose Write the second rule").check();
    await says("2 runs chosen").waitFor();
    await bar()
      .getByRole("button", { name: /Stop runs/ })
      .click();
    await page.getByRole("status").filter({ hasText: "Stopping 2 runs." }).waitFor();
    await expect.poll(() => page.getByTestId("fleet-row").count()).toBe(1);
    expect(first).not.toBe(second);
  });

  it("clears the choice with Escape", async () => {
    await fresh();
    await page.goto(`${server.url}/fleet`);
    await expect.poll(() => page.getByTestId("fleet-row").count()).toBe(1);
    await box(/Choose #1/).focus();
    await page.keyboard.press("Space");
    await says("1 run chosen").waitFor();
    await page.keyboard.press("Escape");
    await bar().waitFor({ state: "detached" });
  });
});
