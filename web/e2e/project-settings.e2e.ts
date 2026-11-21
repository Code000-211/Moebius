/**
 * A project's settings in four tabs, in a real browser: General (its name, whether it is private, intake and the
 * kinds it offers), Repositories (a repository put in the project and taken out again), Members (a private project's,
 * named by an owner) and Columns (a column's script saved from the tab, reached from the board's column menu). The old
 * tabs lead to their new places. Every wait is on something the page shows. The routes are the
 * fake server's own (e2e/fake/pages-a.ts, pages-c.ts and kinds.ts).
 */
import { type Browser, type BrowserContext, chromium, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { pickOption } from "./controls";
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

/** A new fake server and a browser signed in as its owner, on `path`. */
async function open(path: string) {
  server = await startServer();
  context = await browser.newContext({ viewport: { width: 1400, height: 1000 } });
  page = await context.newPage();
  page.setDefaultTimeout(8000);
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  await page.goto(`${server.url}${path}`);
}

/** The body of the last request the fake saw with `method` to `path`. */
function lastBody(method: string, path: string): unknown {
  const sent = (server as Extract<TestServer, { pagesA: unknown }>).pagesA.requests.filter(
    (r) => r.method === method && r.path === path,
  );
  return sent.at(-1)?.body;
}

const tab = (name: string | RegExp) => page.getByRole("tab", { name });

describe.skipIf(usesRealServer)("a project's settings", () => {
  it("opens from the tile, with a tab for each part of the project", async () => {
    await open("/projects");
    await page.getByTestId("project").filter({ hasText: "demo" }).getByRole("link").click();
    await page.waitForURL("**/projects/1/settings");
    await page.getByRole("heading", { name: "demo", level: 1 }).waitFor();
    for (const name of ["General", "Members", "Repositories", "Columns"]) {
      await tab(name).waitFor();
    }
    expect(await page.getByRole("tab").count()).toBe(4);
    // A tab is part of the address.
    await tab("Repositories").click();
    await page.waitForURL("**/projects/1/settings/repositories");
    await page.goto(`${server.url}/projects/1/settings/columns`);
    await tab("Columns").waitFor();
    expect(await tab("Columns").getAttribute("aria-selected")).toBe("true");
  });

  it("sends the tabs that are gone to their new places", async () => {
    await open("/projects/1/settings/workflow");
    await page.waitForURL("**/projects/1/settings/columns");
    await page.goto(`${server.url}/projects/1/settings/plugins`);
    await page.waitForURL("**/projects/1/settings/repositories");
    await page.goto(`${server.url}/projects/1/settings/kinds`);
    await page.waitForURL("**/projects/1/settings");
    await page.goto(`${server.url}/projects/1/settings/import`);
    await page.waitForURL("**/projects/1/settings#import");
    await page.getByRole("form", { name: "Import from Jira" }).waitFor();
  });

  it("opens the script of a column from the board's column menu", async () => {
    await open("/projects/1");
    await page.getByRole("heading", { name: "Backlog" }).click({ button: "right" });
    await page.getByRole("menuitem", { name: "Edit the script" }).click();
    await page.waitForURL("**/projects/1/settings/columns/Backlog?repository=1");
    await tab("Columns").waitFor();
    expect(await tab("Columns").getAttribute("aria-selected")).toBe("true");
    await page.getByRole("textbox", { name: "Script of Backlog" }).waitFor();
  });

  it("turns intake on from General and the project's header gets its Intake tab", async () => {
    await open("/projects/1/settings");
    await page.getByRole("switch", { name: "Intake" }).click();
    await page.getByRole("switch", { name: "Intake", checked: true }).waitFor();
    expect(lastBody("PUT", "/v1/projects/1/intake")).toEqual({ enabled: true });
    await page.getByRole("link", { name: "Open board" }).click();
    await page.getByRole("tab", { name: /^Intake/ }).waitFor();
  });

  it("adds a repository to a project, takes it out again and changes the script of a column", async () => {
    await open("/projects");
    await page.getByRole("button", { name: "New project" }).click();
    const dialog = page.getByRole("dialog");
    await dialog.getByLabel("Name").fill("Storefront");
    await dialog.getByLabel("Key").fill("SHOP");
    await dialog.getByRole("button", { name: "Make project" }).click();
    await dialog.waitFor({ state: "detached" });
    await page.getByTestId("project").filter({ hasText: "Storefront" }).getByRole("link").click();
    await page.waitForURL("**/projects/100/settings");

    await tab("Repositories").click();
    await page.getByText("No repository: this project works on its own files").waitFor();
    await pickOption(page, page.getByRole("button", { name: "Repository to add" }), "demo");
    await page.getByRole("button", { name: "Add to project" }).click();
    const row = page.getByTestId("project-repository").filter({ hasText: "demo" });
    await row.getByText("home: demo").waitFor();
    expect(lastBody("POST", "/v1/projects/100/repositories")).toEqual({ repository: 1 });

    // Its code settings are on the repository's page.
    await row.getByRole("link", { name: "Settings of demo" }).click();
    await page.waitForURL("**/projects/100/repositories/1");
    await page.getByRole("link", { name: "Project settings" }).click();
    await page.waitForURL("**/projects/100/settings");
    await page.goto(`${server.url}/projects/100/settings/repositories`);
    await page.getByRole("button", { name: "Take demo out" }).click();
    await row.waitFor({ state: "detached" });
    await page.getByText("No repository: this project works on its own files").waitFor();

    // The Columns tab: the script of a column is saved from the tab.
    await page.goto(`${server.url}/projects/1/settings/columns`);
    await pickOption(page, page.getByRole("button", { name: "Column" }), "Not started · its own");
    await page.waitForURL("**/projects/1/settings/columns/Not%20started");
    const editor = page.getByRole("textbox", { name: "Script of Not started" });
    await editor.waitFor();
    await editor.click();
    await page.keyboard.press("ControlOrMeta+A");
    await page.keyboard.insertText('-- my own\nmoebius.wait("held")\n');
    await page.getByRole("button", { name: "Save", exact: true }).click();
    await page.getByText("Saved as a commit on the default branch.").waitFor();
    await page.getByText("this project's own").waitFor();
  });

  it("renames a project and makes it private, and its owner names the members", async () => {
    await open("/projects/1/settings");
    await context.request.post(`${server.url}/v1/people`, {
      headers: { "x-moebius": "1" },
      data: { handle: "lea", name: "Lea", role: "member" },
    });
    await page.getByRole("textbox", { name: "Name", exact: true }).fill("Shop");
    await page.getByRole("button", { name: "Save", exact: true }).click();
    await page.getByRole("heading", { name: "Shop", level: 1 }).waitFor();
    expect(lastBody("PATCH", "/v1/projects/1")).toEqual({ name: "Shop" });

    await page.getByRole("switch", { name: "Private" }).click();
    await page.getByRole("switch", { name: "Private", checked: true }).waitFor();
    expect(lastBody("PUT", "/v1/projects/1/access")).toEqual({
      private: true,
      members: ["owner"],
    });

    await tab("Members").click();
    await page.getByLabel("Lea (@lea)").check();
    await page.getByRole("button", { name: "Save members" }).click();
    await page.getByLabel("Lea (@lea)", { exact: false }).waitFor();
    expect(lastBody("PUT", "/v1/projects/1/access")).toEqual({
      private: true,
      members: ["owner", "lea"],
    });
  });

  it("lets a project offer a kind from General", async () => {
    await open("/kinds");
    await page.getByLabel("Name", { exact: true }).fill("Bug");
    await page.getByRole("button", { name: "Add kind" }).click();
    await page.getByTestId("kind-Bug").waitFor();
    await page.goto(`${server.url}/projects/1/settings`);
    // A box is controlled, so it flips when the server has answered.
    await page.getByLabel("Bug").click();
    await page.waitForFunction(
      () => (document.querySelector("input[type=checkbox]") as HTMLInputElement).checked,
    );
    await page.reload();
    expect(await page.getByLabel("Bug").isChecked()).toBe(true);
  });
});
