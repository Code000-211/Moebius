/**
 * The Wiki page's search box and page templates in a real browser, against the fake server and the real one: the box
 * above the tree finds pages by the words of their text and shows the line around the match; Templates makes a page
 * template, and a new page is made from it, its text as the template has it; a deleted template is gone from the
 * choice. Every wait is on something the page shows.
 */
import { type Browser, type BrowserContext, chromium, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { startServer, type TestServer } from "./server";

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

/** A server and a browser signed in as its owner, on the Wiki page. */
async function openWiki() {
  server = await startServer();
  context = await browser.newContext({ viewport: { width: 1400, height: 1000 } });
  page = await context.newPage();
  page.setDefaultTimeout(8000);
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
}

/** Writes a page the way a person's client does, with the cookie and the header the server wants with it. */
async function write(path: string, text: string) {
  const made = await context.request.put(`${server.url}/v1/wiki/pages/${path}`, {
    headers: { "x-moebius": "1" },
    data: { text },
  });
  expect(made.status()).toBe(200);
}

describe("the wiki's search and templates", () => {
  it("finds a page by the words of its text and opens it", async () => {
    await openWiki();
    await write("Runbooks/Refunds", "# Refunds\n\nA refund is paid back within five days.\n");
    await write("Onboarding", "# Onboarding\n\nWelcome to the team.\n");
    await page.goto(`${server.url}/wiki`);
    const tree = page.getByRole("navigation", { name: "Pages of the wiki" });
    await tree.waitFor();

    const box = page.getByRole("searchbox", { name: "Search the wiki" });
    await box.fill("paid back");
    const results = page.getByRole("navigation", { name: "Search results" });
    await results.getByText("1 page", { exact: true }).waitFor();
    await results.getByText("Refunds", { exact: true }).waitFor();
    await results.getByText(/paid back within five days/).waitFor();
    // The tree makes room for the matches.
    expect(await tree.count()).toBe(0);

    await results.getByRole("button", { name: /Refunds/ }).click();
    await page.getByRole("article", { name: "Refunds" }).waitFor();
    expect(page.url()).toContain("page=Runbooks%2FRefunds");
    // Opening a match clears the box and brings the tree back.
    expect(await box.inputValue()).toBe("");
    await page.getByRole("navigation", { name: "Pages of the wiki" }).waitFor();

    // Words no page has say so.
    await box.fill("zeppelin");
    await page.getByText("No page matches").waitFor();
    await box.fill("");
    await page.getByRole("navigation", { name: "Pages of the wiki" }).waitFor();
  });

  it("makes a page template, a page from it, and deletes the template", async () => {
    await openWiki();
    await page.goto(`${server.url}/wiki`);
    await page.getByText("No page yet", { exact: true }).first().waitFor();

    await page.getByRole("button", { name: "Templates", exact: true }).click();
    const dialog = page.getByRole("dialog", { name: "Page templates" });
    await dialog.getByText("No page template yet.").waitFor();
    await dialog.getByLabel("Template name").fill("Runbook");
    await dialog
      .getByLabel("Template text")
      .fill("# Runbook\n\n## When it breaks\n\nCall the owner.");
    await dialog.getByRole("button", { name: "Add template" }).click();
    await dialog.getByRole("list", { name: "Page templates" }).getByText("Runbook").waitFor();

    // An edit keeps the name and changes the text.
    await dialog.getByRole("button", { name: "Edit the template Runbook" }).click();
    await dialog
      .getByLabel("Template text")
      .fill("# Runbook\n\n## When it breaks\n\nCall the owner at once.");
    await dialog.getByRole("button", { name: "Save template" }).click();
    await dialog.getByRole("button", { name: "Add template" }).waitFor();
    await page.keyboard.press("Escape");
    await dialog.waitFor({ state: "detached" });

    // A new page from the template.
    await page.getByRole("button", { name: "New page", exact: true }).first().click();
    const from = page.getByRole("form", { name: "Make a page from a template" });
    await from.getByRole("button", { name: "Template" }).waitFor();
    await from.getByLabel("Name of the page").fill("Payments/Runbook");
    await from.getByRole("button", { name: "Make page" }).click();
    const article = page.getByRole("article", { name: "Runbook" });
    await article.getByText("Call the owner at once.").waitFor();
    expect(page.url()).toContain("page=Payments%2FRunbook");

    // The same name twice is refused with the server's sentence, and nothing is made.
    await page.getByRole("button", { name: "New page", exact: true }).first().click();
    await from.getByLabel("Name of the page").fill("Payments/Runbook");
    await from.getByRole("button", { name: "Make page" }).click();
    await from.getByText(/Couldn't make the page/).waitFor();
    await from.getByText(/exists already|already/i).waitFor();

    // Deleted, the template is no longer offered.
    await page.getByRole("button", { name: "Templates", exact: true }).click();
    await dialog.getByRole("button", { name: "Delete the template Runbook" }).click();
    await dialog.getByText("No page template yet.").waitFor();
    await page.keyboard.press("Escape");
    await dialog.waitFor({ state: "detached" });
    await from.waitFor({ state: "detached" });
  });
});
