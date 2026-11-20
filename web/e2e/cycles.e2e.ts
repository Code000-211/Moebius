/**
 * The Cycles page in a real browser: a project's cycles with their progress, a cycle's own page with the burn-up and
 * the figures above its cards, planning a cycle (making one, editing it, putting a card in it from the card's
 * properties) and closing one so its unfinished cards go on to the next, or out. They run against the fake server of
 * `fake/cycles.ts`; the rules (one open cycle, the same project) are the server's own and tested there. Every wait
 * is on something the page shows, never on time.
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

afterEach(async () => {
  await context.close();
  await server.stop().catch(() => {});
});

async function open(path: string) {
  server = await startServer();
  context = await browser.newContext({ viewport: { width: 1400, height: 1000 } });
  page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  await page.goto(`${server.url}${path}`);
}

const fake = () => {
  if (!("cycles" in server)) throw new Error("this test needs the fake server");
  return server.cycles;
};
const rows = () => page.getByTestId("cycle");
const row = (name: string) => rows().filter({ hasText: name });

describe.skipIf(usesRealServer)("the Cycles page", () => {
  it("is in the sidebar and lists the project's cycles with their progress", async () => {
    await open("/inbox");
    await page
      .getByRole("navigation", { name: "Pages" })
      .getByRole("link", { name: "Cycles" })
      .click();
    await page.getByRole("heading", { name: "Cycles", level: 1 }).waitFor();
    await rows().first().waitFor();
    expect(await rows().count()).toBe(2);
    // Cycle 1 holds a done card of 5 points and an open one of 3.
    await row("Cycle 1").getByText("5 of 8 points · 1 of 2 cards").waitFor();
    expect(await row("Cycle 1").getByRole("progressbar").getAttribute("aria-valuenow")).toBe("63");
    await row("Cycle 2").getByText("0 of 0 cards").waitFor();
  });

  it("opens a cycle to its figures, its burn-up and its cards", async () => {
    await open("/cycles");
    await row("Cycle 1").getByRole("button").click();
    await page.getByRole("heading", { name: "Cycle 1", level: 1 }).waitFor();
    const figure = (name: string) => page.locator("dt", { hasText: name }).locator("xpath=..");
    await figure("Points done").getByText("5 of 8").waitFor();
    await figure("Cards by agents").getByText("1", { exact: true }).waitFor();
    await figure("Tokens per point").getByText("1,000 per point").waitFor();
    await page.getByRole("img", { name: /^Burn-up over 2 days: 5 of 8 points done$/ }).waitFor();
    const cards = page.getByRole("list", { name: "Cards of Cycle 1" });
    await cards.getByText("Add a feature").waitFor();
    await cards.getByText("Fix the login redirect").waitFor();
    expect(await cards.getByRole("listitem").count()).toBe(2);
    // The address keeps the cycle; the way back lists them again.
    expect(new URL(page.url()).searchParams.get("cycle")).toBe("1");
    await page.getByRole("button", { name: "All cycles" }).click();
    await rows().first().waitFor();
  });

  it("makes a cycle, says what the server refuses, and edits it", async () => {
    await open("/cycles");
    await rows().first().waitFor();
    await page.getByRole("button", { name: "New cycle" }).first().click();
    const dialog = page.getByRole("dialog", { name: "New cycle" });
    const make = dialog.getByRole("button", { name: "Make cycle" });
    expect(await make.isDisabled()).toBe(true);
    await dialog.getByLabel("Name").fill("cycle 1");
    await dialog.getByLabel("First day").fill("2026-12-01");
    await dialog.getByLabel("Last day").fill("2026-12-14");
    await make.click();
    await dialog
      .getByRole("alert")
      .getByText(/a cycle named cycle 1 exists already/)
      .waitFor();
    await dialog.getByLabel("Name").fill("Cycle 3");
    await make.click();
    await dialog.waitFor({ state: "detached" });
    await row("Cycle 3").getByText("Dec 1 – Dec 14").waitFor();
    expect(fake().requests.at(-1)).toMatchObject({
      method: "POST",
      path: "/v1/projects/1/cycles",
      body: { name: "Cycle 3", starts_on: "2026-12-01", ends_on: "2026-12-14" },
    });
    // The latest start is first.
    await expect
      .poll(async () => (await rows().first().textContent())?.slice(0, 7))
      .toBe("Cycle 3");

    await row("Cycle 3").getByRole("button").click();
    await page.getByRole("button", { name: "Edit" }).click();
    const edit = page.getByRole("dialog", { name: "Edit cycle" });
    await edit.getByLabel("Name").fill("Cycle three");
    await edit.getByRole("button", { name: "Save" }).click();
    await edit.waitFor({ state: "detached" });
    await page.getByRole("heading", { name: "Cycle three", level: 1 }).waitFor();
  });

  it("closes a cycle and carries its unfinished card to the next one", async () => {
    await open("/cycles?cycle=1");
    await page.getByRole("heading", { name: "Cycle 1", level: 1 }).waitFor();
    await page.getByRole("button", { name: "Close cycle" }).click();
    const dialog = page.getByRole("dialog", { name: "Close Cycle 1" });
    await dialog.getByText(/1 card is not done/).waitFor();
    // The next cycle is chosen already.
    expect(await dialog.getByLabel("Next cycle").inputValue()).toBe("2");
    await dialog.getByRole("button", { name: "Close cycle" }).click();
    await dialog.waitFor({ state: "detached" });
    expect(fake().requests.at(-1)).toMatchObject({
      method: "POST",
      path: "/v1/cycles/1/close",
      body: { carry: "next", to: 2 },
    });
    // The closed cycle keeps both cards and is closed; the open one has the carried card.
    await page.getByRole("button", { name: "Close cycle" }).waitFor({ state: "detached" });
    await page.getByRole("button", { name: "All cycles" }).click();
    await row("Cycle 2").getByText("0 of 3 points · 0 of 1 card").waitFor();
    await row("Cycle 1").getByText("5 of 8 points · 1 of 2 cards").waitFor();
  });

  it("closes a cycle and takes its unfinished card out of any cycle", async () => {
    await open("/cycles?cycle=1");
    await page.getByRole("button", { name: "Close cycle" }).click();
    const dialog = page.getByRole("dialog", { name: "Close Cycle 1" });
    await dialog.getByRole("radio", { name: "Take them out of any cycle" }).check();
    await dialog.getByRole("button", { name: "Close cycle" }).click();
    await dialog.waitFor({ state: "detached" });
    expect(fake().requests.at(-1)).toMatchObject({ body: { carry: "out" } });
    await page.getByRole("button", { name: "All cycles" }).click();
    await row("Cycle 2").getByText("0 of 0 cards").waitFor();
  });

  it("shows a card's cycle in its properties and puts it in another", async () => {
    await open("/board/1");
    await page.getByRole("heading", { name: "demo", level: 1 }).waitFor();
    await page.getByRole("group", { name: /^#1 Add a feature$/ }).click();
    const properties = page.getByRole("region", { name: /^Properties of #/ });
    await properties.getByRole("button", { name: "Cycle Cycle 1" }).click();
    await page.getByRole("menuitemradio", { name: "Cycle 2" }).click();
    await properties.getByRole("button", { name: "Cycle Cycle 2" }).waitFor();
    expect(fake().requests.at(-1)).toMatchObject({
      method: "PUT",
      path: "/v1/cards/1/cycle",
      body: { cycle: 2 },
    });
    // The page of cycles counts it now: one card of 5 points moved over.
    await page.goto(`${server.url}/cycles`);
    await row("Cycle 2").getByText("5 of 5 points · 1 of 1 card").waitFor();
    await row("Cycle 1").getByText("0 of 3 points · 0 of 1 card").waitFor();
  });

  it("deletes a cycle and leaves its cards", async () => {
    await open("/cycles?cycle=2");
    await page.getByRole("heading", { name: "Cycle 2", level: 1 }).waitFor();
    await page.getByRole("button", { name: "Delete" }).click();
    await page.getByRole("dialog").getByRole("button", { name: "Delete" }).click();
    await rows().first().waitFor();
    await row("Cycle 2").waitFor({ state: "detached" });
    expect(await rows().count()).toBe(1);
  });
});
