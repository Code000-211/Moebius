/**
 * Mentions in a real browser (ADR 0018): what concerns the person is an item of the Inbox under its Mentions tab with
 * a read mark of their own, and `@` completes the handles of people in a comment. The fake server tells the owner
 * what the real one tells on a mention; the real server's own telling is tested in Rust. Every wait is on something
 * the page shows.
 */
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

async function open(path: string) {
  server = await startServer();
  context = await browser.newContext({ viewport: { width: 1400, height: 1000 } });
  page = await context.newPage();
  page.setDefaultTimeout(8000);
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  await page.goto(`${server.url}${path}`);
}

const tell = (notice: Parameters<FakeServer["notify"]>[1]) => {
  if (!server.notify) throw new Error("this test needs the fake server");
  server.notify("owner", notice);
};

describe.skipIf(usesRealServer)("mentions", () => {
  it("shows what concerns the person under Mentions, reads it for them alone and opens the card", async () => {
    await open("/inbox");
    await page.getByRole("tab", { name: /^Mentions 0/ }).waitFor();
    const inbox = page
      .getByRole("navigation", { name: "Pages" })
      .getByRole("link", { name: /Inbox/ });
    const waiting = Number(/\d+/.exec((await inbox.textContent()) ?? "")?.[0]);
    tell({ kind: "assigned", card_id: 7, title: "Ship the first release", body: "Given to you" });
    tell({
      kind: "mentioned",
      by: "lea",
      card_id: 7,
      title: "Ship the first release",
      body: "@owner can you look?",
    });
    await page.getByRole("tab", { name: /^Mentions 2/ }).click();
    const notices = page.getByTestId("notice");
    await notices.nth(1).waitFor();
    // Newest first: the mention, then the assignment; both unread, and the sidebar counts them.
    expect(await notices.first().getAttribute("data-kind")).toBe("mentioned");
    await notices.first().getByText("@lea mentioned you: @owner can you look?").waitFor();
    expect(await page.locator("[data-unread]").count()).toBe(2);
    await inbox.getByText(String(waiting + 2), { exact: true }).waitFor();

    // Choosing the assignment reads it and opens the card.
    await notices.nth(1).click();
    await page.waitForURL(/\/board\/1\?card=\d+/);
    await page.goBack();
    await page.getByRole("tab", { name: /^Mentions 1/ }).waitFor();
    expect(await page.locator("[data-unread]").count()).toBe(1);
  });

  it("says when nothing mentions the person", async () => {
    await open("/inbox?kind=mentions");
    await page.getByText("Nothing mentions you").waitFor();
  });

  it("completes the handle of a person after an @ in a comment", async () => {
    await open("/board/1");
    await page.getByTestId("card-7").click();
    const box = page.getByRole("combobox", { name: "Write a comment" });
    await box.fill("Please look, @ow");
    const option = page.getByRole("option", { name: /@owner/ });
    await option.waitFor();
    await box.press("Enter");
    await page.getByRole("listbox").waitFor({ state: "detached" });
    expect(await box.inputValue()).toBe("Please look, @owner ");
    // An address names no one: nothing is offered.
    await box.fill("write me@ow");
    await page.getByRole("listbox").waitFor({ state: "detached" });
  });
});
