/**
 * The web app in a real browser against a server (the fake of fakeServer.ts, or a real one: see server.ts): signing in
 * with the token and with the `?login=` link, the sidebar, the command bar, and the banner for a server that does not
 * answer. Every wait is on something the page shows, never on time.
 */
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type Browser, type BrowserContext, chromium, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { AGENT_TOKEN } from "./fakeServer";
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

/** A new server and a new browser context (no cookies) for every test, so the tests do not depend on each other. */
async function fresh() {
  server = await startServer();
  context = await browser.newContext();
  page = await context.newPage();
}

/** The folders of the projects a real server was given, removed after each test. */
const directories: string[] = [];

afterEach(async () => {
  await context.close();
  await server.stop().catch(() => {});
  for (const directory of directories.splice(0))
    rmSync(directory, { recursive: true, force: true });
});

/**
 * Gives a real server a project, as the fake starts with one: the sidebar lists the Board and Files of the chosen
 * project, and a server with none lists "All projects" in their place.
 */
async function withProject() {
  if (!usesRealServer) return;
  const directory = mkdtempSync(join(tmpdir(), "moebius-e2e-repo-"));
  directories.push(directory);
  const response = await context.request.post(`${server.url}/v1/repositories`, {
    headers: { "X-Moebius": "1", Authorization: `Bearer ${server.token}` },
    data: { name: "demo", url: directory },
  });
  expect(response.ok(), await response.text()).toBe(true);
}

const pages = () => page.getByRole("navigation", { name: "Pages" });
const path = () => new URL(page.url()).pathname;

describe("signing in", () => {
  it("shows the sign-in page, refuses a wrong token and accepts the right one", async () => {
    await fresh();
    await page.goto(server.url);
    await page.getByRole("heading", { name: "Moebius" }).waitFor();

    await page.getByLabel("Token").fill("wrong");
    await page.getByRole("button", { name: "Sign in" }).click();
    await page.getByRole("alert").filter({ hasText: "not accepted" }).waitFor();

    await page.getByLabel("Token").fill(server.token);
    await page.getByRole("button", { name: "Sign in" }).click();
    await pages().waitFor();
    await page.waitForURL("**/inbox");

    // The cookie keeps the browser signed in across a reload.
    await page.reload();
    await pages().waitFor();
    expect(path()).toBe("/inbox");
  });

  it("signs a person in with the access token an owner made for them", async () => {
    await fresh();
    const added = await fetch(`${server.url}/v1/people`, {
      method: "POST",
      headers: { authorization: `Bearer ${server.token}`, "content-type": "application/json" },
      body: JSON.stringify({ handle: "lea", name: "Lea", role: "member" }),
    });
    expect(added.status).toBe(200);
    const { token } = (await added.json()) as { token: { token: string } };
    await page.goto(server.url);
    await page.getByLabel("Token").fill(token.token);
    await page.getByRole("button", { name: "Sign in" }).click();
    await pages().waitFor();
    await pages().getByRole("link", { name: "Settings" }).click();
    await page.getByText("Signed in as Lea").waitFor();
  });

  // The real server's run tokens exist only while a run does.
  it.skipIf(usesRealServer)("refuses the run token of an agent with its own sentence", async () => {
    await fresh();
    await page.goto(server.url);
    await page.getByLabel("Token").fill(AGENT_TOKEN);
    await page.getByRole("button", { name: "Sign in" }).click();
    await page.getByRole("alert").filter({ hasText: "run token" }).waitFor();
  });

  it("signs in through the ?login= link and takes the code out of the address", async () => {
    await fresh();
    await page.goto(server.loginUrl);
    await pages().waitFor();
    const url = new URL(page.url());
    expect(url.search).toBe("");
    expect(url.searchParams.has("login")).toBe(false);
    expect(url.pathname).toBe("/inbox");
  });

  it("shows the sign-in page with a reason when the link was used already", async () => {
    await fresh();
    await page.goto(server.loginUrl);
    await pages().waitFor();
    const other = await browser.newContext();
    try {
      const second = await other.newPage();
      await second.goto(server.loginUrl);
      await second.getByRole("alert").filter({ hasText: "sign-in link" }).waitFor();
      expect(new URL(second.url()).search).toBe("");
    } finally {
      await other.close();
    }
  });

  it("signs out from Settings", async () => {
    await fresh();
    await page.goto(server.loginUrl);
    await pages().waitFor();
    await pages().getByRole("link", { name: "Settings" }).click();
    await page.getByRole("heading", { name: "General", level: 1 }).waitFor();
    await page.getByRole("button", { name: "Sign out" }).click();
    // The way back in: the sign-in form's own field, named exactly, since Settings has several that say Token.
    await page.getByLabel("Token", { exact: true }).waitFor();
  });
});

describe("the shell", () => {
  it("opens a page from the sidebar and marks it as the current one", async () => {
    await fresh();
    await page.goto(server.loginUrl);
    await pages().waitFor();
    await pages().getByRole("link", { name: "Fleet" }).first().click();
    await page.waitForURL("**/fleet");
    await page.getByRole("heading", { name: "Fleet", level: 1 }).waitFor();
    expect(
      await pages().getByRole("link", { name: "Fleet" }).first().getAttribute("aria-current"),
    ).toBe("page");
  });

  it("lists the everyday pages, then the pages Settings holds, and the boards", async () => {
    await fresh();
    await withProject();
    await page.goto(server.loginUrl);
    await pages().waitFor();
    for (const name of [
      "Inbox",
      "Fleet",
      "Board",
      "Files",
      "Automations",
      "Workflows",
      "Retro",
      "Experiments",
      "Decisions",
      "Settings",
    ]) {
      // An entry may carry a count after its name ("Inbox 1", "Fleet 2").
      await pages()
        .getByRole("link", { name: new RegExp(`^${name}\\b`) })
        .waitFor();
    }
    await pages().getByText("Automate", { exact: true }).first().waitFor();
    // Improve is one row that folds over its pages, and keeps its state over a reload.
    await pages().getByRole("button", { name: "Collapse Improve" }).click();
    await pages().getByRole("button", { name: "Expand Improve" }).waitFor();
    expect(
      await pages()
        .getByRole("link", { name: /^Retro/ })
        .count(),
    ).toBe(0);
    await page.reload();
    await pages().getByRole("button", { name: "Expand Improve" }).waitFor();
    await pages().getByRole("button", { name: "Expand Improve" }).click();
    await pages()
      .getByRole("link", { name: /^Retro/ })
      .waitFor();
    // The list of every project is reached from the switcher.
    await pages().getByRole("button", { name: "Change project" }).click();
    await page.getByRole("menuitem", { name: "All projects" }).waitFor();
    await page.keyboard.press("Escape");
    // The pages that set the server up are not listed beside the everyday ones: Settings opens them.
    expect(await pages().getByRole("link", { name: "Models" }).count()).toBe(0);
    await pages().getByRole("link", { name: "Settings" }).click();
    await page.waitForURL("**/settings");
    for (const name of [
      "General",
      "Models",
      "Plugins",
      "Plugin settings",
      "Terminal",
      "Ports",
      "Usage",
      "Rules",
      "Agents",
      "Skills",
      "MCP",
    ]) {
      await pages()
        .getByRole("link", { name: new RegExp(`^${name}\\b`) })
        .waitFor();
    }
    // The way back is a row under Search, and it goes where the person last was.
    await pages().getByRole("link", { name: "Back" }).click();
    await page.waitForURL("**/inbox");
    if (!usesRealServer) {
      await pages()
        .getByRole("link", { name: /^Board/ })
        .click();
      await page.waitForURL("**/board/1");
      await page.getByRole("heading", { name: "demo" }).waitFor();
      // The chosen project stays chosen on other pages and after a reload.
      await pages()
        .getByRole("link", { name: /^Fleet/ })
        .click();
      await page.reload();
      await pages()
        .getByRole("link", { name: /^Board/ })
        .waitFor();
    }
  });

  it("opens a page by its address and keeps it on reload", async () => {
    await fresh();
    await page.goto(server.loginUrl);
    await pages().waitFor();
    await page.goto(`${server.url}/usage`);
    await page.getByRole("heading", { name: "Usage" }).waitFor();
    await page.reload();
    await page.getByRole("heading", { name: "Usage" }).waitFor();
  });

  it("collapses to a rail and remembers it", async () => {
    await fresh();
    await page.goto(server.loginUrl);
    await pages().waitFor();
    await page.getByRole("button", { name: "Collapse the sidebar" }).click();
    await page.getByRole("button", { name: "Expand the sidebar" }).waitFor();
    await page.reload();
    await page.getByRole("button", { name: "Expand the sidebar" }).waitFor();
  });

  it("goes online and tells so", async () => {
    await fresh();
    await page.goto(server.loginUrl);
    await page.getByTestId("connection").getByText("Online").waitFor();
  });

  it("keeps the shell standing when a page's file cannot be fetched", async () => {
    await fresh();
    await page.goto(server.loginUrl);
    await pages().waitFor();
    // The file of the Fleet page never arrives, as when the server stops answering between the click and the fetch,
    // or a newer version replaced the file this tab was told about.
    let blocked = 0;
    await page.route("**/assets/FleetPage-*.js", (route) => {
      blocked += 1;
      void route.abort("connectionrefused");
    });
    await pages().getByRole("link", { name: "Fleet" }).click();
    const line = page.getByRole("alert").filter({ hasText: "The page could not be loaded" });
    await line.getByRole("button", { name: "Reload" }).waitFor();
    // What is not missing is still there to reload from: the sidebar and the connection, not a stack trace.
    await pages().getByRole("link", { name: "Inbox" }).waitFor();
    await page.getByTestId("connection").waitFor();
    expect(blocked).toBeGreaterThan(0);
  });
});

describe("a page that is still being read", () => {
  it("shows the shape of what is coming while the server answers", async () => {
    await fresh();
    // The answer is held rather than slowed, so the test waits on the page and not on the clock.
    let release = () => {};
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    let asked = 0;
    await page.route("**/v1/snapshot", async (route) => {
      asked += 1;
      await held;
      await route.continue();
    });
    try {
      await page.goto(server.loginUrl);
      // The page stands while it is read, heading and all, rather than being a spinner in an empty window.
      await page.getByRole("heading", { name: "Inbox", level: 1 }).waitFor();
      const shape = page.getByRole("status").filter({ hasText: "Reading the inbox" });
      await shape.waitFor();
      // The state is still said, in words nobody has to look at, and the rows are drawn rather than written.
      expect(await shape.locator("[aria-hidden='true']").count()).toBeGreaterThan(3);
      expect(await page.locator(".animate-spin").count()).toBe(0);
    } finally {
      release();
    }
    // The shape gives way to what it stood for.
    expect(asked).toBeGreaterThan(0);
    await page.getByRole("status").filter({ hasText: "Reading the inbox" }).waitFor({
      state: "detached",
    });
  });
});

describe("the command bar", () => {
  it("opens with Ctrl+K, filters the pages and opens the one chosen", async () => {
    await fresh();
    await page.goto(server.loginUrl);
    await pages().waitFor();
    await page.keyboard.press("Control+k");
    const input = page.getByPlaceholder("Search…");
    await input.waitFor();
    // The sheet holds both the field and the list: the field stays the height of a control rather than growing to
    // fill the sheet, and the list ends inside the sheet rather than running past it.
    const sheet = await page.getByRole("dialog").boundingBox();
    const field = await input.boundingBox();
    const list = await page.getByRole("listbox").boundingBox();
    if (!sheet || !field || !list) throw new Error("the dialog did not open as one sheet");
    expect(field.height).toBeLessThanOrEqual(56);
    expect(list.y + list.height).toBeLessThanOrEqual(sheet.y + sheet.height + 1);
    await input.fill("retro");
    await page.getByRole("option", { name: "Retro" }).waitFor();
    expect(await page.getByRole("option").count()).toBe(1);
    await page.keyboard.press("Enter");
    await page.waitForURL("**/retro");
    await input.waitFor({ state: "detached" });
    await page.getByRole("heading", { name: "Retro", level: 1 }).waitFor();
  });

  it("opens from the sidebar's search and closes with Escape", async () => {
    await fresh();
    await page.goto(server.loginUrl);
    await pages().waitFor();
    await page.getByRole("button", { name: "Search" }).click();
    const input = page.getByPlaceholder("Search…");
    await input.waitFor();
    await input.fill("zzzz");
    await page.getByText("Nothing matches.").waitFor();
    await page.keyboard.press("Escape");
    await input.waitFor({ state: "detached" });
  });
});

describe("a server that does not answer", () => {
  // The tests do not restart a real server.
  it.skipIf(usesRealServer)(
    "shows the banner, keeps the page, and recovers when the server is back",
    async () => {
      await fresh();
      if (!server.restart) throw new Error("the fake server restarts");
      await page.goto(server.loginUrl);
      await pages().waitFor();
      await pages().getByRole("link", { name: "Fleet" }).click();
      await page.waitForURL("**/fleet");
      // A page is fetched when it opens, so it has to be on the screen before the server goes away: stopping the
      // server mid-fetch loses the page itself, which is not the thing this test is about.
      await page.getByRole("heading", { name: "Fleet" }).waitFor();

      await server.stop();
      const banner = page.getByRole("alert").filter({ hasText: "Cannot reach the server" });
      await banner.waitFor();
      await page.getByTestId("connection").getByText("Reconnecting").waitFor();

      await server.restart();
      await banner.getByRole("button", { name: "Retry now" }).click();
      await banner.waitFor({ state: "detached" });
      await page.getByTestId("connection").getByText("Online").waitFor();
    },
  );

  it("shows a screen with a retry when the first answer never comes", async () => {
    await fresh();
    // The page loads but the API refuses connections, as when the server restarts under an open tab.
    await page.route("**/v1/session", (route) => route.abort("connectionrefused"));
    await page.goto(server.loginUrl);
    await page.getByRole("heading", { name: "Cannot reach the server" }).waitFor();
    await page.unroute("**/v1/session");
    await page.getByRole("button", { name: "Retry now" }).click();
    await page.getByLabel("Token").waitFor();
  });
});
