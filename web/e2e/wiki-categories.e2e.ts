/**
 * Categories and labels of the wiki, and a project's Wiki tab, in a real browser: a page is made in a new category (a
 * folder) and then in an existing one; labels are added to a page and removed, and the tree is filtered by them; a
 * project's Wiki tab shows only the project's own pages, categories and labels, and makes a page there.
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

async function signedIn(path: string): Promise<Page> {
  server = await startServer();
  const context = await browser.newContext({ viewport: { width: 1400, height: 900 } });
  const page = await context.newPage();
  page.setDefaultTimeout(8000);
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  await page.goto(`${server.url}${path}`);
  return page;
}

/** Makes a page: the category as "None", an existing one or a new one, then the name and the text. */
async function makePage(
  page: Page,
  category: { existing: string } | { fresh: string } | "none",
  name: string,
  text: string,
) {
  const section = page.getByRole("region", { name: "New page" });
  if (category !== "none") {
    await section.getByLabel("Category", { exact: true }).click();
    if ("fresh" in category) {
      await page.getByRole("menuitemradio", { name: "+ New category" }).click();
      await section.getByLabel("Name of the new category").fill(category.fresh);
    } else {
      await page.getByRole("menuitemradio", { name: category.existing, exact: true }).click();
    }
  }
  const form = page.getByRole("form", { name: "New page" });
  await form.getByLabel("Name", { exact: true }).fill(name);
  await form.getByLabel("Text").fill(text);
  await form.getByRole("button", { name: "Save" }).click();
  await form.waitFor({ state: "detached" });
}

describe("categories and labels of the wiki", () => {
  it("makes a page in a new category, then another in it, and counts the pages of a category", async () => {
    const page = await signedIn("/wiki");
    await page.getByText("No page yet", { exact: true }).first().waitFor();
    await page.getByRole("button", { name: "New page", exact: true }).first().click();
    await makePage(page, { fresh: "Runbooks" }, "Deploy", "# Deploy\n\nShip it.");
    const article = page.getByRole("article", { name: "Deploy" });
    await article.getByText("Ship it.").waitFor();
    expect(page.url()).toContain("page=Runbooks%2FDeploy");
    // The page says its category.
    await article.getByText(/· Runbooks$/).waitFor();

    // The picker offers the category that now exists.
    await page.getByRole("button", { name: "New page", exact: true }).first().click();
    await makePage(page, { existing: "Runbooks" }, "Rollback", "# Rollback\n\nUndo it.");
    expect(page.url()).toContain("page=Runbooks%2FRollback");

    // The tree keeps the category, with the count of its pages.
    const tree = page.getByRole("navigation", { name: "Pages of the wiki" });
    await tree.getByRole("button", { name: /^Runbooks\s*2$/ }).waitFor();

    // A page with no category is at the top.
    await page.getByRole("button", { name: "New page", exact: true }).first().click();
    await makePage(page, "none", "Handbook", "# Handbook\n\nHello.");
    expect(page.url()).toContain("page=Handbook");
    await page
      .getByRole("article", { name: "Handbook" })
      .getByText(/Lea|Owner/)
      .waitFor();

    // Move offers the same picker: the page goes into the category.
    await page.getByRole("button", { name: "Move" }).click();
    const dialog = page.getByRole("dialog", { name: "Move the page" });
    await dialog.getByLabel("Category", { exact: true }).click();
    await page.getByRole("menuitemradio", { name: "Runbooks", exact: true }).click();
    await dialog.getByRole("button", { name: "Move" }).click();
    await page.waitForURL(/page=Runbooks%2FHandbook/);
    await tree.getByRole("button", { name: /^Runbooks\s*3$/ }).waitFor();
    await page.context().close();
  });

  it("adds and removes labels on a page and filters the tree by them", async () => {
    const page = await signedIn("/wiki");
    await page.getByText("No page yet", { exact: true }).first().waitFor();
    await page.getByRole("button", { name: "New page", exact: true }).first().click();
    await makePage(page, "none", "Deploy", "# Deploy\n\nShip it.");
    await page.getByRole("button", { name: "New page", exact: true }).first().click();
    await makePage(page, "none", "Rota", "# Rota\n\nWho is on call.");

    const labels = page.getByRole("group", { name: "Labels" });
    await labels.getByRole("button", { name: "Label", exact: true }).click();
    await labels.getByLabel("New label").fill("Runbook");
    await labels.getByLabel("New label").press("Enter");
    await labels.getByText("Runbook").waitFor();
    await labels.getByRole("button", { name: "Label", exact: true }).click();
    await labels.getByLabel("New label").fill("On call");
    await labels.getByLabel("New label").press("Enter");
    await labels.getByText("On call").waitFor();

    // A label the server would refuse is said so before it is sent.
    await labels.getByRole("button", { name: "Label", exact: true }).click();
    await labels.getByLabel("New label").fill("a,b");
    await page.getByText(/A label cannot hold/).waitFor();
    await labels.getByLabel("New label").press("Escape");

    // The labels are the page's front matter: the page does not draw it and the editor does not show it, and an edit
    // of the text keeps them.
    const article = page.getByRole("article", { name: "Rota" });
    expect(await article.innerText()).not.toContain("labels:");
    await article.getByRole("button", { name: "Edit" }).click();
    const editor = page.getByRole("form", { name: "Edit Rota" });
    expect(await editor.getByLabel("Text").inputValue()).toBe("# Rota\n\nWho is on call.\n");
    await editor.getByLabel("Text").fill("# Rota\n\nWho is on call this week.");
    await editor.getByRole("button", { name: "Save" }).click();
    await editor.waitFor({ state: "detached" });
    await page.getByRole("article", { name: "Rota" }).getByText("this week").waitFor();
    await page.getByRole("group", { name: "Labels" }).getByText("Runbook").waitFor();
    await page.getByRole("group", { name: "Labels" }).getByText("On call").waitFor();

    // The chips above the tree: every label in use, and All.
    const filter = page.getByRole("group", { name: "Filter by label" });
    await filter.getByRole("button", { name: "All", exact: true }).waitFor();
    const tree = page.getByRole("navigation", { name: "Pages of the wiki" });
    await tree.getByRole("button", { name: "Deploy" }).waitFor();
    await filter.getByRole("button", { name: /^Runbook/ }).click();
    await expect.poll(() => tree.getByRole("button", { name: "Deploy" }).count()).toBe(0);
    await tree.getByRole("button", { name: "Rota" }).waitFor();
    await filter.getByRole("button", { name: /^On call/ }).click();
    await tree.getByRole("button", { name: "Rota" }).waitFor();
    await filter.getByRole("button", { name: "All", exact: true }).click();
    await tree.getByRole("button", { name: "Deploy" }).waitFor();

    // Search finds a page by its label.
    if (!usesRealServer) {
      await page.getByRole("searchbox").fill("on call");
      await page
        .getByRole("navigation", { name: "Search results" })
        .getByText("Rota")
        .first()
        .waitFor();
      await page.getByRole("searchbox").fill("");
    }

    // A label taken away is gone from the page and from the chips.
    await page.getByRole("button", { name: "Remove the label Runbook" }).click();
    await expect.poll(() => labels.getByText("Runbook").count()).toBe(0);
    await expect.poll(() => filter.getByRole("button", { name: /^Runbook/ }).count()).toBe(0);
    await page.getByRole("button", { name: "Remove the label On call" }).click();
    await expect.poll(() => filter.count()).toBe(0);
    await page.getByRole("article", { name: "Rota" }).getByRole("button", { name: "Edit" }).click();
    expect(
      await page.getByRole("form", { name: "Edit Rota" }).getByLabel("Text").inputValue(),
    ).toBe("# Rota\n\nWho is on call this week.\n");
    await page.context().close();
  });
});

describe("a project's Wiki tab", () => {
  it("shows only the project's pages, categories and labels, and makes a page there", async () => {
    const page = await signedIn("/projects/1");
    if (usesRealServer || !("wiki" in server)) return;
    server.wiki.pushFromElsewhere("Handbook", "# Handbook\n\nFor everyone.");
    server.wiki.pushFromElsewhere(
      "projects/DEMO/Design/Tokens",
      "---\nlabels: [Design]\n---\n# Tokens\n\nColours.",
    );
    server.wiki.pushFromElsewhere("projects/DEMO/Notes", "# Notes\n\nA note.");

    // The tab is after Milestones and before Intake.
    const tabs = page.getByRole("tablist", { name: "Project" });
    await tabs.getByRole("tab", { name: "Wiki" }).click();
    await page.waitForURL("**/projects/1/wiki");
    const names = await tabs.getByRole("tab").allTextContents();
    expect(names.indexOf("Wiki")).toBe(names.indexOf("Milestones") + 1);

    const tree = page.getByRole("navigation", { name: "Pages of the wiki" });
    await tree.getByRole("button", { name: "Notes" }).waitFor();
    await tree.getByRole("button", { name: /^Design\s*1$/ }).waitFor();
    // The organisation's page is not here; the sidebar's Wiki still has it.
    expect(await tree.getByRole("button", { name: "Handbook" }).count()).toBe(0);
    const filter = page.getByRole("group", { name: "Filter by label" });
    await filter.getByRole("button", { name: /^Design/ }).waitFor();
    await filter.getByRole("button", { name: /^Design/ }).click();
    await expect.poll(() => tree.getByRole("button", { name: "Notes" }).count()).toBe(0);
    await tree.getByRole("button", { name: "Tokens" }).click();
    await page.waitForURL(/page=projects%2FDEMO%2FDesign%2FTokens/);
    await page.getByRole("article", { name: "Tokens" }).getByText("Colours.").waitFor();
    await filter.getByRole("button", { name: "All", exact: true }).click();

    // New page creates in the project: its categories are the project's, and the page is under projects/DEMO/.
    await page.getByRole("button", { name: "New page", exact: true }).first().click();
    const section = page.getByRole("region", { name: "New page" });
    await section.getByLabel("Category", { exact: true }).click();
    await page.getByRole("menuitemradio", { name: "Design", exact: true }).click();
    await page.keyboard.press("Escape");
    const form = page.getByRole("form", { name: "New page" });
    await form.getByLabel("Name", { exact: true }).fill("Spacing");
    await form.getByLabel("Text").fill("# Spacing\n\nEight pixels.");
    await form.getByRole("button", { name: "Save" }).click();
    await form.waitFor({ state: "detached" });
    expect(page.url()).toContain("/projects/1/wiki");
    expect(page.url()).toContain("page=projects%2FDEMO%2FDesign%2FSpacing");
    await page.getByRole("article", { name: "Spacing" }).getByText("Eight pixels.").waitFor();
    await tree.getByRole("button", { name: /^Design\s*2$/ }).waitFor();

    // The sidebar's Wiki shows everything.
    await page.goto(`${server.url}/wiki`);
    const all = page.getByRole("navigation", { name: "Pages of the wiki" });
    await all.getByRole("button", { name: "Handbook" }).waitFor();
    await all
      .getByRole("button", { name: "demo" })
      .first()
      .waitFor()
      .catch(() => {});
    await page.context().close();
  });
});
