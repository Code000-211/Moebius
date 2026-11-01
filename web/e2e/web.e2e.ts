/**
 * The web app in a real browser against a server (the fake of fakeServer.ts, or a real one: see server.ts): signing in
 * with the token and with the `?login=` link, the sidebar, the command bar, and the banner for a server that does not
 * answer. Every wait is on something the page shows, never on time.
 */
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

afterEach(async () => {
  await context.close();
  await server.stop().catch(() => {});
});

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

  it("signs out from the account menu", async () => {
    await fresh();
    await page.goto(server.loginUrl);
    await pages().waitFor();
    await page.getByRole("button", { name: "Account" }).click();
    await page.getByRole("menuitem", { name: "Sign out" }).click();
    await page.getByLabel("Token").waitFor();
  });
});

describe("the shell", () => {
  it("opens a page from the sidebar, with a placeholder for what is not built", async () => {
    await fresh();
    await page.goto(server.loginUrl);
    await pages().waitFor();
    await pages().getByRole("link", { name: "Agents" }).click();
    await page.waitForURL("**/agents");
    await page.getByRole("heading", { name: "Agents" }).waitFor();
    await page.getByText("coming in the web app soon").waitFor();
    expect(await pages().getByRole("link", { name: "Agents" }).getAttribute("aria-current")).toBe(
      "page",
    );
  });

  it("lists the same groups as the desktop sidebar and the boards of the snapshot", async () => {
    await fresh();
    await page.goto(server.loginUrl);
    await pages().waitFor();
    for (const name of ["Inbox", "Chat", "Fleet", "Automations", "Retro", "Models", "Settings"]) {
      // An entry may carry a count after its name ("Inbox 1", "Fleet 1 live").
      await pages()
        .getByRole("link", { name: new RegExp(`^${name}\\b`) })
        .waitFor();
    }
    for (const group of ["Projects", "Build", "Improve", "Setup"]) {
      await pages().getByText(group, { exact: true }).first().waitFor();
    }
    if (!usesRealServer) {
      await pages().getByRole("link", { name: /demo/ }).click();
      await page.waitForURL("**/board/1");
      await page.getByRole("heading", { name: "demo" }).waitFor();
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
});

describe("the command bar", () => {
  it("opens with Ctrl+K, filters the pages and opens the one chosen", async () => {
    await fresh();
    await page.goto(server.loginUrl);
    await pages().waitFor();
    await page.keyboard.press("Control+k");
    const input = page.getByPlaceholder("Go to a page…");
    await input.waitFor();
    await input.fill("retro");
    await page.getByRole("option", { name: "Retro" }).waitFor();
    expect(await page.getByRole("option").count()).toBe(1);
    await page.keyboard.press("Enter");
    await page.waitForURL("**/retro");
    await input.waitFor({ state: "detached" });
    await page.getByRole("heading", { name: "Retro" }).waitFor();
  });

  it("opens from the sidebar's search and closes with Escape", async () => {
    await fresh();
    await page.goto(server.loginUrl);
    await pages().waitFor();
    await page.getByRole("button", { name: "Search pages" }).click();
    const input = page.getByPlaceholder("Go to a page…");
    await input.waitFor();
    await input.fill("zzzz");
    await page.getByText("No page matches.").waitFor();
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

      await server.stop();
      const banner = page.getByRole("alert").filter({ hasText: "Cannot reach the server" });
      await banner.waitFor();
      await page.getByRole("heading", { name: "Fleet" }).waitFor();
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
