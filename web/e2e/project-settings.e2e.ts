/**
 * A project's settings in tabs, in a real browser: General (its name and whether it is private), Repositories (a
 * repository put in the project and taken out again), Members (a private project's, named by an owner), Workflow (a
 * column's script saved from the tab), Kinds and Plugins. Every wait is on something the page shows. The routes are the
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
    for (const name of ["General", "Repositories 1", "Members 1", "Workflow", "Kinds", "Plugins"]) {
      await tab(name).waitFor();
    }
    // A tab is part of the address.
    await tab("Kinds").click();
    await page.waitForURL("**/projects/1/settings/kinds");
    await page.getByText("The kinds of issues this project offers").waitFor();
    await page.goto(`${server.url}/projects/1/settings/workflow`);
    await tab("Workflow").waitFor();
    expect(await tab("Workflow").getAttribute("aria-selected")).toBe("true");
  });

  it("adds a repository to a project, takes it out again and changes its workflow", async () => {
    await open("/projects");
    await page.getByRole("button", { name: "New project" }).click();
    const dialog = page.getByRole("dialog");
    await dialog.getByLabel("Name").fill("Storefront");
    await dialog.getByLabel("Key").fill("SHOP");
    await dialog.getByRole("button", { name: "Make project" }).click();
    await dialog.waitFor({ state: "detached" });
    await page.getByTestId("project").filter({ hasText: "Storefront" }).getByRole("link").click();
    await page.waitForURL("**/projects/100/settings");

    await tab("Repositories 0").click();
    await page.getByText("No repository: this project works on its own files").waitFor();
    await pickOption(page, page.getByRole("button", { name: "Repository to add" }), "demo");
    await page.getByRole("button", { name: "Add to project" }).click();
    const row = page.getByTestId("project-repository").filter({ hasText: "demo" });
    await row.getByText("home: demo").waitFor();
    await tab("Repositories 1").waitFor();
    expect(lastBody("POST", "/v1/projects/100/repositories")).toEqual({ repository: 1 });

    // Its code settings are on the repository's page.
    await row.getByRole("link", { name: "Settings of demo" }).click();
    await page.waitForURL("**/projects/100/repositories/1");
    await page.getByRole("link", { name: "Project settings" }).click();
    await page.waitForURL("**/projects/100/settings");
    await page.goto(`${server.url}/projects/100/settings/repositories`);
    await page.getByRole("button", { name: "Take demo out" }).click();
    await row.waitFor({ state: "detached" });
    await tab("Repositories 0").waitFor();

    // The workflow: the script of a column is saved from the tab.
    await page.goto(`${server.url}/projects/1/settings/workflow`);
    await pickOption(page, page.getByRole("button", { name: "Column" }), "Not started · its own");
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
    await page.getByRole("textbox", { name: "Name" }).fill("Shop");
    await page.getByRole("button", { name: "Save", exact: true }).click();
    await page.getByRole("heading", { name: "Shop", level: 1 }).waitFor();
    expect(lastBody("PATCH", "/v1/projects/1")).toEqual({ name: "Shop" });

    await page.getByRole("switch", { name: "Private" }).click();
    await page.getByRole("switch", { name: "Private", checked: true }).waitFor();
    expect(lastBody("PUT", "/v1/projects/1/access")).toEqual({
      private: true,
      members: ["owner"],
    });

    await tab("Members 1").click();
    await page.getByLabel("Lea (@lea)").check();
    await page.getByRole("button", { name: "Save members" }).click();
    await tab("Members 2").waitFor();
    expect(lastBody("PUT", "/v1/projects/1/access")).toEqual({
      private: true,
      members: ["owner", "lea"],
    });
  });

  it("lets a project offer a kind from its Kinds tab", async () => {
    await open("/kinds");
    await page.getByLabel("Name", { exact: true }).fill("Bug");
    await page.getByRole("button", { name: "Add kind" }).click();
    await page.getByTestId("kind-Bug").waitFor();
    await page.goto(`${server.url}/projects/1/settings/kinds`);
    // A box is controlled, so it flips when the server has answered.
    await page.getByLabel("Bug").click();
    await page.waitForFunction(
      () => (document.querySelector("input[type=checkbox]") as HTMLInputElement).checked,
    );
    await page.reload();
    expect(await page.getByLabel("Bug").isChecked()).toBe(true);
  });

  it("says plugins ask nothing of a project with no repository", async () => {
    await open("/projects");
    await page.getByRole("button", { name: "New project" }).click();
    const dialog = page.getByRole("dialog");
    await dialog.getByLabel("Name").fill("Docs");
    await dialog.getByLabel("Key").fill("DOCS");
    await dialog.getByRole("button", { name: "Make project" }).click();
    await dialog.waitFor({ state: "detached" });
    await page.goto(`${server.url}/projects/100/settings/plugins`);
    await page.getByText("Plugins ask their settings of a repository").waitFor();
  });
});
