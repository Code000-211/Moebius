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
 * Gives a real server a project, as the fake starts with one: the sidebar lists it under Projects.
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

const pages = () => page.getByRole("navigation", { name: "Pages", exact: true });
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

  it("keeps the sign-in across a closed browser with Stay signed in, and drops it without", async () => {
    await fresh();
    await page.goto(server.url);
    const stay = page.getByLabel("Stay signed in");
    expect(await stay.isChecked()).toBe(true);
    await page.getByLabel("Token").fill(server.token);
    await page.getByRole("button", { name: "Sign in" }).click();
    await pages().waitFor();
    // A cookie with an expiry survives the browser being closed; reload and the stored state both stay signed in.
    const kept = (await context.cookies()).find((c) => c.name === "moebius_session");
    expect(kept?.expires).toBeGreaterThan(Date.now() / 1000);
    await page.reload();
    await pages().waitFor();

    await context.clearCookies();
    await page.goto(server.url);
    await stay.uncheck();
    await page.getByLabel("Token").fill(server.token);
    await page.getByRole("button", { name: "Sign in" }).click();
    await pages().waitFor();
    // Without an expiry the cookie ends with the browser session: a new browser starts without it.
    const session = (await context.cookies()).find((c) => c.name === "moebius_session");
    expect(session?.expires).toBe(-1);
    await page.reload();
    await pages().waitFor();
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
    await page
      .getByRole("navigation", { name: "Settings" })
      .getByRole("link", { name: "Profile" })
      .click();
    await page.getByText("@lea").waitFor();
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
    await page
      .getByRole("navigation", { name: "Settings" })
      .getByRole("link", { name: "Profile" })
      .click();
    await page.getByRole("heading", { name: "Profile", level: 2 }).waitFor();
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

  it("lists the places of the standard, in order, and nothing else", async () => {
    await fresh();
    await withProject();
    await page.goto(server.loginUrl);
    await pages().waitFor();
    await pages().getByRole("link", { name: "Wiki" }).waitFor();
    // A link may carry a count after its name ("Inbox 1", "Fleet 2 running"); the projects are the fake's.
    const names = await pages()
      .getByRole("link")
      .evaluateAll((links) =>
        links.map((link) => (link.textContent ?? "").replace(/\d+( running)?$/, "")),
      );
    const projects = ["demo"];
    expect(names).toEqual([
      "Inbox",
      "My issues",
      "Fleet",
      "Projects",
      ...projects,
      "Wiki",
      "Cycles",
      "Views",
      "More",
      "Settings",
    ]);
    expect(
      await pages()
        .getByRole("button")
        .evaluateAll((all) => all.map((b) => b.ariaLabel)),
    ).toEqual(["Search"]);
  });

  it("keeps every row 36px high and draws no key hint, mask or live box", async () => {
    await fresh();
    await withProject();
    await page.goto(server.loginUrl);
    await pages().getByRole("link", { name: "Wiki" }).waitFor();
    await pages().getByRole("link", { name: "demo", exact: true }).waitFor();
    const heights = await pages()
      .getByRole("link")
      .evaluateAll((links) =>
        links
          .filter((link) => link.textContent !== "Projects")
          .map((link) => link.getBoundingClientRect().height),
      );
    expect(heights.length).toBeGreaterThan(8);
    for (const height of heights) expect(height).toBe(36);
    const inside = await pages().evaluate((nav) => ({
      kbd: nav.querySelectorAll("kbd").length,
      masks: [nav, ...nav.querySelectorAll("*")].filter((element) => {
        const style = getComputedStyle(element);
        return (
          (style.maskImage !== "" && style.maskImage !== "none") ||
          (style.webkitMaskImage !== "" && style.webkitMaskImage !== "none")
        );
      }).length,
      live: nav.querySelectorAll("[data-testid=connection], [data-testid=live]").length,
    }));
    expect(inside).toEqual({ kbd: 0, masks: 0, live: 0 });
  });

  it("opens Projects, a project's board, the Wiki and Settings from the sidebar", async () => {
    await fresh();
    await withProject();
    await page.goto(server.loginUrl);
    await pages().waitFor();
    await pages().getByRole("link", { name: "Projects", exact: true }).click();
    await page.waitForURL("**/projects");
    await pages().getByRole("link", { name: "Wiki" }).click();
    await page.waitForURL("**/wiki");
    await pages().getByRole("link", { name: "Settings" }).click();
    await page.waitForURL("**/settings");
    // The sidebar is the same list on a Settings page, with Settings chosen and no way back to leave it.
    await pages().getByRole("link", { name: "Wiki" }).waitFor();
    await expect
      .poll(() => pages().getByRole("link", { name: "Settings" }).getAttribute("aria-current"))
      .toBe("page");
    await expect.poll(() => pages().getByRole("link", { name: "Back" }).count()).toBe(0);
    await expect.poll(() => pages().getByRole("link", { name: "Models" }).count()).toBe(0);
    if (!usesRealServer) {
      await pages().getByRole("link", { name: "demo", exact: true }).click();
      await page.waitForURL("**/projects/1?repository=1");
      await page.getByRole("heading", { name: "demo" }).waitFor();
      await expect
        .poll(() =>
          pages().getByRole("link", { name: "demo", exact: true }).getAttribute("aria-current"),
        )
        .toBe("page");
    }
  });

  /** The sidebar as the group names and rows it draws, in the order of the DOM. */
  const sidebarLines = () =>
    pages().evaluate((nav) =>
      Array.from(nav.querySelectorAll("a, div")).flatMap((node) => {
        if (node.tagName === "A") {
          // The rows of Projects are the repositories' boards, the others are the places of the table.
          const href = node.getAttribute("href") ?? "";
          return href.startsWith("/projects/")
            ? []
            : [(node.textContent ?? "").replace(/\d+( running)?$/, "")];
        }
        return node.className.includes("text-sidebar-group") && !node.closest("a")
          ? [`# ${node.textContent}`]
          : [];
      }),
    );

  it.skipIf(usesRealServer)(
    "lists, for an owner, Inbox to Chat, Projects, Workspace, Build, Insight and Settings in that order",
    async () => {
      await fresh();
      await page.goto(server.loginUrl);
      await pages().waitFor();
      await expect
        .poll(sidebarLines)
        .toEqual([
          "Inbox",
          "My issues",
          "Fleet",
          "Chat",
          "Projects",
          "# Workspace",
          "Wiki",
          "Views",
          "# Build",
          "Agents",
          "Rules",
          "# Insight",
          "Insights",
          "Improve",
          "Get started",
          "Settings",
        ]);
      await pages().getByRole("link", { name: "Get started" }).click();
      await page.waitForURL("**/start");
      await page.getByRole("heading", { name: "Get started", level: 1 }).waitFor();
    },
  );

  it.skipIf(usesRealServer)("leaves Build and Insight out for a member", async () => {
    await fresh();
    const added = await fetch(`${server.url}/v1/people`, {
      method: "POST",
      headers: { authorization: `Bearer ${server.token}`, "content-type": "application/json" },
      body: JSON.stringify({ handle: "lea", name: "Lea", role: "member" }),
    });
    const { token } = (await added.json()) as { token: { token: string } };
    await page.goto(server.url);
    await page.getByLabel("Token").fill(token.token);
    await page.getByRole("button", { name: "Sign in" }).click();
    await pages().waitFor();
    await expect
      .poll(sidebarLines)
      .toEqual([
        "Inbox",
        "My issues",
        "Fleet",
        "Chat",
        "Projects",
        "# Workspace",
        "Wiki",
        "Views",
        "Get started",
        "Settings",
      ]);
  });

  it("answers /more with the Inbox and /quick with the window of its own, outside the shell", async () => {
    await fresh();
    await page.goto(server.loginUrl);
    await pages().waitFor();
    await page.goto(`${server.url}/more`);
    await page.waitForURL("**/inbox");
    await page.goto(`${server.url}/quick`);
    await page.getByTestId("quick-window").waitFor();
    expect(await pages().count()).toBe(0);
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
    // What is not missing is still there to reload from: the sidebar not a stack trace.
    await pages().getByRole("link", { name: "Inbox" }).waitFor();
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
      await expect.poll(() => shape.locator("[aria-hidden='true']").count()).toBeGreaterThan(3);
      await expect.poll(() => page.locator(".animate-spin").count()).toBe(0);
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
    await expect.poll(() => page.getByRole("option").count()).toBe(1);
    await page.keyboard.press("Enter");
    await page.waitForURL("**/retro");
    await input.waitFor({ state: "detached" });
    await page.getByRole("heading", { name: "Retro" }).first().waitFor();
  });

  it("draws rows of 36px with an icon and no key hint", async () => {
    await fresh();
    await page.goto(server.loginUrl);
    await pages().waitFor();
    await page.keyboard.press("Control+k");
    const dialog = page.getByRole("dialog", { name: "Search" });
    await dialog.getByRole("option").first().waitFor();
    await expect.poll(() => dialog.locator("kbd").count()).toBe(0);
    const row = await dialog.getByRole("option").first().boundingBox();
    expect(row?.height).toBe(36);
    for (const option of await dialog.getByRole("option").all()) {
      await expect.poll(() => option.locator("svg").count()).toBeGreaterThan(0);
    }
  });

  it("sends the typed text to the agent on Tab, and does nothing on Tab with no text", async () => {
    await fresh();
    await page.goto(server.loginUrl);
    await pages().waitFor();
    const agent = page.getByRole("complementary", { name: "Moebius agent" });
    await page.keyboard.press("Control+k");
    const input = page.getByPlaceholder("Search…");
    await input.waitFor();
    await page.keyboard.press("Tab");
    await expect.poll(() => input.isVisible()).toBe(true);
    await expect.poll(() => agent.count()).toBe(0);
    await input.fill("What is this page?");
    await page.keyboard.press("Tab");
    await input.waitFor({ state: "detached" });
    await agent.waitFor();
    if (usesRealServer) {
      // A new real server has no model for the agent: it says so in its own sentence, and no message is shown as said.
      await agent.getByText("the Moebius agent needs a model", { exact: false }).waitFor();
      await expect.poll(() => agent.getByText("Nothing said yet").count()).toBe(1);
    } else {
      await agent.getByText("You said: What is this page?").first().waitFor();
    }
  });

  it("starts a new conversation with the agent from the palette", async () => {
    await fresh();
    await page.goto(server.loginUrl);
    await pages().waitFor();
    const agent = page.getByRole("complementary", { name: "Moebius agent" });
    await page.keyboard.press("Control+k");
    await page.getByPlaceholder("Search…").fill("new conversation");
    await page.getByRole("option", { name: "New conversation" }).click();
    await agent.waitFor();
    await expect.poll(() => agent.getByText("You said:").count()).toBe(0);
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

      await server.restart();
      await banner.getByRole("button", { name: "Retry now" }).click();
      await banner.waitFor({ state: "detached" });
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
