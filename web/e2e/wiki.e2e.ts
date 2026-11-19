/**
 * The Wiki page in a real browser: an owner makes a page in a folder, reads it with its contents at the right, edits
 * it with a preview, reads its history and the page as the first commit left it, and moves it to another folder. On
 * the fake server a page pushed to the wiki's repository from elsewhere then shows without a reload. Every wait is on
 * something the page shows.
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

describe("the Wiki page", () => {
  it("makes, edits and reads a page and its history", async () => {
    server = await startServer();
    const context = await browser.newContext();
    const page = await context.newPage();
    page.setDefaultTimeout(8000);
    await page.goto(server.loginUrl);
    await page.getByRole("navigation", { name: "Pages" }).waitFor();
    await page.goto(`${server.url}/wiki`);
    await page.getByText("No page yet", { exact: true }).first().waitFor();

    // A new page, in a folder the name makes.
    await page.getByRole("button", { name: "New page", exact: true }).first().click();
    const form = page.getByRole("form", { name: "New page" });
    await form.getByLabel("Name").fill("Team/On call");
    await form
      .getByLabel("Text")
      .fill("# On call\n\nWho answers at night.\n\n## Rotation\n\nWeekly.");
    await form
      .getByRole("region", { name: "Preview" })
      .getByText("Who answers at night.")
      .waitFor();
    await form.getByRole("button", { name: "Save" }).click();

    const article = page.getByRole("article", { name: "On call" });
    await article.getByText("Who answers at night.").waitFor();
    expect(page.url()).toContain("page=Team%2FOn+call");
    const tree = page.getByRole("navigation", { name: "Pages of the wiki" });
    await tree.getByRole("button", { name: "On call" }).waitFor();
    const contents = page.getByRole("navigation", { name: "On this page" });
    await contents.getByRole("button", { name: "Rotation" }).waitFor();

    // An edit, with a message for its commit.
    await article.getByRole("button", { name: "Edit" }).click();
    const editor = page.getByRole("form", { name: "Edit Team/On call" });
    await editor.getByLabel("Text").fill("# On call\n\nThe person of the week answers.");
    await editor.getByLabel("What changed").fill("Say who answers");
    await editor.getByRole("button", { name: "Save" }).click();
    await editor.waitFor({ state: "detached" });
    await article.getByText("The person of the week answers.").waitFor();

    // The history, newest first, and the page as its first commit left it.
    await article.getByRole("button", { name: "History" }).click();
    const history = page.getByRole("list", { name: "History of Team/On call" });
    await history.getByText("Say who answers").waitFor();
    const rows = history.getByRole("button");
    expect(await rows.count()).toBe(2);
    await rows.nth(1).getByText("Add Team/On call").waitFor();
    await rows.nth(1).click();
    await page.getByText(/An older version: Add Team\/On call/).waitFor();
    await page
      .getByRole("article", { name: "On call" })
      .getByText("Who answers at night.")
      .waitFor();
    await page.getByRole("button", { name: "Read the page as it is" }).click();
    await page
      .getByRole("article", { name: "On call" })
      .getByText("The person of the week answers.")
      .waitFor();

    // Moved to another folder, it keeps its history.
    await page.getByRole("button", { name: "Move" }).click();
    const dialog = page.getByRole("dialog", { name: "Move the page" });
    await dialog.getByLabel("Path").fill("Handbook/On call");
    await dialog.getByRole("button", { name: "Move" }).click();
    await page.waitForURL(/page=Handbook%2FOn\+call/);
    await page.getByRole("button", { name: "History" }).click();
    await page
      .getByRole("list", { name: "History of Handbook/On call" })
      .getByText("Move Team/On call to Handbook/On call")
      .waitFor();

    if (!usesRealServer && "wiki" in server) {
      server.wiki.pushFromElsewhere("Runbooks/Refunds", "# Refunds\n\nFrom elsewhere.");
      await tree.getByRole("button", { name: "Runbooks" }).click();
      await tree.getByRole("button", { name: "Refunds" }).click();
      await page.getByRole("article", { name: "Refunds" }).getByText("From elsewhere.").waitFor();

      // The palette finds a page by a word of its text and opens it.
      await page.goto(`${server.url}/wiki`);
      await page.getByRole("navigation", { name: "Pages" }).waitFor();
      await page.keyboard.press("Control+k");
      const palette = page.getByRole("dialog", { name: "Search" });
      await palette.getByPlaceholder("Search…").fill("elsewhere");
      await palette.getByRole("option", { name: /Refunds/ }).click();
      await page.waitForURL(/page=Runbooks%2FRefunds/);
      await page.getByRole("article", { name: "Refunds" }).getByText("From elsewhere.").waitFor();
    }
    await context.close();
  });
});
