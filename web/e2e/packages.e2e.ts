/**
 * The package sheet in a real browser, against the fake server (e2e/fake/packages.ts): opened from the Plugins page on
 * the Moebius registries and from an agent's Extensions on the pi.dev catalog. A plugin is searched, installed,
 * updated and removed; a workflow is proposed and the link lands on its change in the Inbox; a Pi package is proposed
 * for the agent the sheet was opened from; a registry that cannot be read and a refused install are said in the sheet;
 * the sheet leaves the page it was opened from uncovered; and the events refresh what the page lists. Every wait is on
 * something the page shows, never on time.
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

async function open(path: string, width = 1280) {
  server = await startServer();
  server.pagesB?.use();
  context = await browser.newContext({ viewport: { width, height: 900 } });
  page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  await page.goto(`${server.url}${path}`);
}

function fake(): FakeServer["packages"] {
  if (!server.packages) throw new Error("these tests run against the fake server");
  return server.packages;
}

const sheet = () => page.getByRole("dialog", { name: "Package sheet" });
const report = () => sheet().getByTestId("package-report");

/** The requests of one method to one path the fake received. */
const requests = (method: string, path: string) =>
  fake().calls.filter((call) => call.method === method && call.path === path);

async function openRegistries() {
  await page.getByRole("button", { name: "Browse packages" }).first().click();
  await sheet().waitFor();
}

/** Searches the sheet and opens the package of that name. */
async function openPackage(search: string, name = search) {
  await sheet().getByLabel("Search the registries").fill(search);
  await sheet().getByTestId("list-card").filter({ hasText: name }).first().click();
  await sheet().getByTestId("package-detail").waitFor();
}

describe.skipIf(usesRealServer)("the Plugins page and the Moebius registries", () => {
  it("lists the installed packages on the page, with the update that is waiting", async () => {
    await open("/plugins");
    const installed = page.getByTestId("installed-packages");
    await installed.getByTestId("installed-package").nth(1).waitFor();
    const rows = installed.getByTestId("installed-package");
    expect(await rows.nth(0).textContent()).toContain("gate");
    expect(await rows.nth(0).textContent()).toContain("Installed 1.0.0");
    expect(await rows.nth(0).textContent()).toContain("Update to 1.1.0");
    expect(await rows.nth(1).textContent()).toContain("guard-pack");
    expect(
      await rows
        .nth(1)
        .getByRole("button", { name: /^Update/ })
        .count(),
    ).toBe(0);

    // The update is made from the page itself, and the page reads the list again.
    await rows.nth(0).getByRole("button", { name: "Update gate" }).click();
    await installed.getByText("Updated gate to 1.1.0.").waitFor();
    await rows.nth(0).getByText("Installed 1.1.0").waitFor();
    expect(requests("POST", "/v1/packages/gate/update")).toHaveLength(1);
  });

  it("searches, filters by kind, and opens a package with its versions", async () => {
    await open("/plugins");
    await openRegistries();
    await sheet().getByTestId("list-card").nth(5).waitFor();
    // Results show the name, kind, version and whether the package is installed or has an update.
    const gate = sheet().getByTestId("list-card").filter({ hasText: "Holds cards marked wip." });
    expect(await gate.textContent()).toContain("Plugin");
    expect(await gate.textContent()).toContain("1.1.0");
    expect(await gate.textContent()).toContain("Update to 1.1.0");
    await sheet().getByRole("button", { name: "Rules", exact: true }).click();
    await sheet().getByTestId("list-card").filter({ hasText: "house-rules" }).waitFor();
    expect(await sheet().getByTestId("list-card").count()).toBe(1);
    await sheet().getByRole("button", { name: "All", exact: true }).click();
    await openPackage("pad", "pad-launcher");
    const versions = await sheet().getByRole("list", { name: "Versions" }).textContent();
    expect(versions).toContain("0.4.0");
    expect(versions).toContain("Works with this Moebius");
  });

  it("installs a plugin at once, says it needs a restart, then removes another after asking", async () => {
    await open("/plugins");
    await openRegistries();
    await openPackage("pad-launcher");
    await sheet().getByRole("button", { name: "Install", exact: true }).click();
    await report().getByText("Installed pad-launcher 0.4.0.").waitFor();
    await report().getByText("Restart the server for it to take effect.").waitFor();
    expect(requests("POST", "/v1/packages")[0]?.body).toEqual({
      name: "pad-launcher",
      registry: "moebius-plugins",
    });
    // The page behind the sheet lists it without being reloaded.
    await page
      .getByTestId("installed-packages")
      .getByTestId("installed-package")
      .filter({ hasText: "pad-launcher" })
      .waitFor();

    await sheet().getByRole("tab", { name: "Installed" }).click();
    const row = sheet().getByTestId("installed-package").filter({ hasText: "guard-pack" });
    await row.getByRole("button", { name: "Remove guard-pack" }).click();
    // The first press only asks.
    expect(requests("DELETE", "/v1/packages/guard-pack")).toHaveLength(0);
    await row.getByRole("button", { name: "Confirm removing guard-pack" }).click();
    await report().getByText("Removed guard-pack.").waitFor();
    await sheet().getByTestId("installed-package").filter({ hasText: "guard-pack" }).waitFor({
      state: "detached",
    });
    expect(fake().installedNames()).not.toContain("guard-pack");
  });

  it("updates an installed plugin from the sheet's Installed tab", async () => {
    await open("/plugins");
    await openRegistries();
    await sheet().getByRole("tab", { name: "Installed" }).click();
    await sheet().getByRole("button", { name: "Update gate" }).click();
    await report().getByText("Updated gate to 1.1.0.").waitFor();
    await sheet().getByRole("button", { name: "Update gate" }).waitFor({ state: "detached" });
  });

  it("shows the plugin's own error when it does not run", async () => {
    await open("/plugins");
    await openRegistries();
    await openPackage("shaky");
    await sheet().getByRole("button", { name: "Install", exact: true }).click();
    await report()
      .getByText("shaky does not run: exited with status 1 before sending its manifest")
      .waitFor();
  });

  it("proposes a workflow as a change and the link lands on it in the Inbox", async () => {
    await open("/plugins");
    await openRegistries();
    await openPackage("review-flow");
    await sheet().getByText("Nothing is written until a person merges it.").waitFor();
    await sheet().getByRole("button", { name: "Propose install" }).click();
    await report().getByRole("link", { name: "See it in the Inbox" }).click();
    await page.waitForURL(/\/inbox\?change=41$/);
    const item = page.getByTestId("inbox-item");
    await item.waitFor();
    expect(await item.count()).toBe(1);
    expect(await item.textContent()).toContain("Install review-flow");
    // The sheet gave way to the Inbox and leaves its button.
    await page.getByRole("button", { name: "Open the package sheet" }).waitFor();
    expect(await sheet().count()).toBe(0);
  });

  it("says a registry that cannot be read beside the results of the others", async () => {
    await open("/plugins");
    fake().problem = { registry: "acme", message: "cannot read index.toml: not valid" };
    await openRegistries();
    await sheet().getByText("The registry acme could not be read.", { exact: false }).waitFor();
    await sheet().getByText("cannot read index.toml: not valid", { exact: false }).waitFor();
    await sheet().getByTestId("list-card").first().waitFor();
  });

  it("shows a refused install with its sentence and keeps the package open", async () => {
    await open("/plugins");
    await openRegistries();
    await openPackage("from-the-future");
    await sheet().getByRole("button", { name: "Install", exact: true }).click();
    await sheet()
      .getByRole("alert")
      .getByText("from-the-future 9.0.0 works with Moebius >=9.0, and this is Moebius 0.0.0-fake")
      .waitFor();
    await sheet().getByRole("button", { name: "Install", exact: true }).waitFor();

    fake().refuseNext("POST", "/v1/packages", 403, "only a person can install a plugin");
    await sheet().getByRole("button", { name: "Install", exact: true }).click();
    await sheet()
      .getByRole("alert")
      .getByText("this session is not a person's", { exact: false })
      .waitFor();
  });

  const contentRight = () =>
    page.locator("main").evaluate((main) => {
      const rect = main.getBoundingClientRect();
      return rect.right - Number.parseFloat(getComputedStyle(main).paddingRight);
    });

  it("lies over the page on a window under 1536px, which keeps its width, and minimises to a button", async () => {
    await open("/plugins", 1280);
    const before = await contentRight();
    await openRegistries();
    const box = await sheet().boundingBox();
    expect(await contentRight()).toBe(before);
    expect(before).toBeGreaterThan(box?.x ?? 0);

    await page.keyboard.press("Escape");
    await sheet().waitFor({ state: "hidden" });
    // Focus returns to the button that opens it again, and the search is where it was left.
    await page.getByRole("button", { name: "Open the package sheet" }).click();
    await sheet().waitFor();
    await sheet().getByTestId("list-card").first().waitFor();
    // Minimise with its own control as well.
    await sheet().getByRole("button", { name: "Minimise the package sheet" }).click();
    await sheet().waitFor({ state: "hidden" });
  });

  it("leaves the page its room on a window of 1536px or more", async () => {
    await open("/plugins", 1600);
    const before = await contentRight();
    await openRegistries();
    const box = await sheet().boundingBox();
    expect(await contentRight()).toBeLessThanOrEqual(box?.x ?? 0);
    expect(await contentRight()).toBeLessThan(before);
  });

  it("shares its slot with the floating chat: opening one minimises the other, and both buttons stay in view", async () => {
    await open("/plugins");
    await openRegistries();
    const chat = page.getByRole("dialog", { name: "Floating chat" });
    // The chat's button is not under the open sheet.
    const opener = page.getByRole("button", { name: "Open the floating chat" });
    const sheetBox = await sheet().boundingBox();
    const openerBox = await opener.boundingBox();
    expect(openerBox?.y ?? 0).toBeGreaterThanOrEqual((sheetBox?.y ?? 0) + (sheetBox?.height ?? 0));
    await opener.click();
    await chat.waitFor();
    await sheet().waitFor({ state: "hidden" });
    const back = page.getByRole("button", { name: "Open the package sheet" });
    await back.click();
    await sheet().waitFor();
    await chat.waitFor({ state: "hidden" });
    // The search it had is where it was left.
    await sheet().getByTestId("list-card").first().waitFor();
  });

  it("reads again what the events say changed", async () => {
    await open("/plugins");
    await page.getByTestId("installed-packages").getByTestId("installed-package").first().waitFor();
    const reads = () => requests("GET", "/v1/packages").length;
    for (const [kind, data] of [
      ["package_installed", { name: "gate", version: "1.1.0", kind: "plugin" }],
      ["package_removed", { name: "gate", kind: "plugin" }],
      ["package_update_available", { name: "gate", version: "1.2.0" }],
      ["extension_update_available", { name: "pi-notes", version: "0.4.0" }],
      ["package_check_error", { registry: "acme", message: "unreadable" }],
    ] as const) {
      const before = reads();
      server.broadcast?.({ kind, data });
      await expect.poll(reads, { message: kind }).toBeGreaterThan(before);
    }
  });
});

describe.skipIf(usesRealServer)("an agent's Extensions and the pi.dev catalog", () => {
  it("proposes a Pi package for the agent and lands on the change", async () => {
    await open("/agents?agent=Reviewer", 1600);
    await page.getByRole("button", { name: "Browse Pi packages" }).click();
    await sheet().waitFor();
    await sheet().getByText("Pi packages from pi.dev, for Reviewer").waitFor();

    // The editor is not under the sheet: its Extensions can be scrolled to a place left of it.
    await page.getByRole("button", { name: "Add extension" }).scrollIntoViewIfNeeded();
    const editor = await page.getByRole("button", { name: "Add extension" }).boundingBox();
    const box = await sheet().boundingBox();
    expect((editor?.x ?? 0) + (editor?.width ?? 0)).toBeLessThanOrEqual(box?.x ?? 0);

    const row = sheet().getByTestId("list-card").filter({ hasText: "pi-web-access" });
    await row.waitFor();
    expect(await row.textContent()).toContain("12.4k a week");
    await sheet().getByRole("button", { name: "Skill", exact: true }).click();
    await sheet().getByTestId("list-card").filter({ hasText: "@acme/pi-notes" }).waitFor();
    expect(await sheet().getByTestId("list-card").count()).toBe(1);
    await sheet().getByRole("button", { name: "All", exact: true }).click();

    await row.click();
    await sheet().getByText("npm would run postinstall", { exact: false }).waitFor();
    await sheet().getByRole("button", { name: "Propose install for Reviewer" }).click();
    await report().getByText("It is named for Reviewer.").waitFor();
    expect(requests("POST", "/v1/extensions/packages")[0]?.body).toEqual({
      source: "npm:pi-web-access",
      agent: "Reviewer",
    });
    await report().getByRole("link", { name: "See it in the Inbox" }).click();
    await page.waitForURL(/\/inbox\?change=41$/);
    const item = page.getByTestId("inbox-item");
    await item.waitFor();
    expect(await item.textContent()).toContain("Install pi-web-access");
  });

  it("shows an update waiting, with its changelog, and updates all", async () => {
    await open("/agents?agent=Reviewer");
    fake().waitForUpdate("pi-notes", "0.3.0", "0.4.0");
    await page.getByRole("button", { name: "Browse Pi packages" }).click();
    await sheet().getByRole("tab", { name: "Installed" }).click();
    const row = sheet().getByTestId("installed-package").filter({ hasText: "pi-notes" });
    await row.getByText("Update to 0.4.0").waitFor();
    await row.getByRole("button", { name: "Update pi-notes" }).click();
    await sheet()
      .getByRole("region", { name: "Changelog of pi-notes" })
      .getByText("Notes survive a restart.")
      .waitFor();

    await sheet().getByRole("button", { name: "Update all" }).click();
    await report().getByText("pi-broken could not be updated.", { exact: false }).waitFor();
  });

  it("reads and proposes an installed package from its own source, not npm's", async () => {
    await open("/agents?agent=Reviewer");
    await page.getByRole("button", { name: "Browse Pi packages" }).click();
    await sheet().getByRole("tab", { name: "Installed" }).click();
    await sheet().getByRole("button", { name: "Details of pi-gitted" }).click();
    await sheet()
      .getByTestId("package-detail")
      .getByText("git:github.com/acme/pi-gitted")
      .waitFor();
    expect(fake().calls.some((call) => call.path.includes("info?source=npm%3Api-gitted"))).toBe(
      false,
    );
  });

  it("proposes removing a package without asking twice, since a person merges it", async () => {
    await open("/agents?agent=Reviewer");
    await page.getByRole("button", { name: "Browse Pi packages" }).click();
    await sheet().getByRole("tab", { name: "Installed" }).click();
    await sheet().getByRole("button", { name: "Remove pi-notes" }).click();
    await report().getByText("Proposed removing pi-notes 0.3.0.", { exact: false }).waitFor();
    expect(requests("DELETE", "/v1/extensions/packages/pi-notes")).toHaveLength(1);
  });

  it("switches the sheet's catalog when it is opened from the other place", async () => {
    await open("/agents?agent=Reviewer");
    await page.getByRole("button", { name: "Browse Pi packages" }).click();
    await sheet().getByText("pi-web-access").waitFor();
    // Moving by the sidebar keeps the page, and so the sheet, where it is.
    await page
      .getByRole("navigation", { name: "Pages" })
      .getByRole("link", { name: "Plugins" })
      .click();
    await page.getByRole("button", { name: "Browse packages" }).first().click();
    await sheet().getByText("Plugins, workflows, rules and skills from the registries").waitFor();
    await sheet().getByText("review-flow").waitFor();
  });
});
