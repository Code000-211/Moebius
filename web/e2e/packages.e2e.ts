/**
 * The package sheet in a real browser, against the fake server (e2e/fake/packages.ts): opened from the Plugins page on
 * the Moebius registries, and the Extensions tab of an agent's page on the pi.dev catalog. A plugin is searched, installed,
 * updated and removed; a workflow is proposed and the link lands on its change in the Inbox; a Pi package is proposed
 * for the agent whose page it is added from; a registry that cannot be read and a refused install are said in the sheet;
 * the sheet leaves the page it was opened from uncovered; and the events refresh what the page lists. Every wait is on
 * something the page shows, never on time.
 */
import { type Browser, type BrowserContext, chromium, type Locator, type Page } from "playwright";
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
  await page.getByRole("navigation", { name: "Pages", exact: true }).waitFor();
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
    await expect
      .poll(() =>
        rows
          .nth(1)
          .getByRole("button", { name: /^Update/ })
          .count(),
      )
      .toBe(0);

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
    await expect.poll(() => sheet().getByTestId("list-card").count()).toBe(1);
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
    await expect.poll(() => item.count()).toBe(1);
    expect(await item.textContent()).toContain("Install review-flow");
    // The sheet gave way to the Inbox, and leaves no button floating over it.
    await sheet().waitFor({ state: "hidden" });
    await expect
      .poll(() => page.getByRole("button", { name: "Open the package sheet" }).count())
      .toBe(0);
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

  /**
   * Where a sheet lies in the page's surface, from its layout rather than its drawn box, which moves while it slides
   * in: the gaps to the surface's top, right and bottom, and its width.
   */
  const place = (sheet: Locator) =>
    sheet.evaluate((element) => {
      const box = element as HTMLElement;
      const surface = box.offsetParent as HTMLElement;
      return {
        top: box.offsetTop,
        right: surface.clientWidth - box.offsetLeft - box.offsetWidth,
        bottom: surface.clientHeight - box.offsetTop - box.offsetHeight,
        width: box.offsetWidth,
      };
    });

  const measure = () =>
    page.locator("main").evaluate((main) => {
      const rect = main.getBoundingClientRect();
      return rect.right - Number.parseFloat(getComputedStyle(main).paddingRight);
    });
  // Right after the page opens its padding may not be computed yet, which reads as NaN: the edge is taken once it is a
  // number.
  const contentRight = async () => {
    let right = Number.NaN;
    await expect
      .poll(async () => {
        right = await measure();
        return Number.isFinite(right);
      })
      .toBe(true);
    return right;
  };

  it("lies over the right of the page on a window under 1536px, which keeps its width", async () => {
    await open("/plugins", 1280);
    const before = await contentRight();
    await openRegistries();
    const box = await sheet().boundingBox();
    expect(await contentRight()).toBe(before);
    expect(before).toBeGreaterThan(box?.x ?? 0);
    // It is flush with the page's right and bottom, starts below the page header (48px, where the window's buttons
    // are), and is wider than the 272px column the pages once had.
    expect(await place(sheet())).toEqual({
      top: 48,
      right: 0,
      bottom: 0,
      width: expect.any(Number),
    });
    expect((await place(sheet())).width).toBeGreaterThan(272);
    // No round button floats over the page.
    await expect
      .poll(() => page.getByRole("button", { name: "Open the package sheet" }).count())
      .toBe(0);

    await page.keyboard.press("Escape");
    await sheet().waitFor({ state: "hidden" });
    // Opened again from the page, the search is where it was left.
    await openRegistries();
    await sheet().getByTestId("list-card").first().waitFor();
    // Close with its own control as well.
    await sheet().getByRole("button", { name: "Close the package sheet" }).click();
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

describe.skipIf(usesRealServer)("an agent's Extensions tab and the pi.dev catalog", () => {
  const found = () => page.getByTestId("package-result");
  const installed = () => page.getByTestId("installed-package");
  const told = () => page.getByTestId("package-report");

  it("lists what the agent has, and nothing of another agent's", async () => {
    await open("/agents/Coder?tab=extensions");
    const row = installed().filter({ hasText: "pi-notes" });
    await row.waitFor();
    expect(await row.textContent()).toContain("0.3.0");
    expect(await installed().count()).toBe(1);
    // The tab is a page, not a sheet.
    expect(await sheet().count()).toBe(0);
  });

  it("proposes a Pi package for the agent and lands on the change", async () => {
    await open("/agents/Reviewer?tab=extensions");
    const row = found().filter({ hasText: "pi-web-access" });
    await row.waitFor();
    expect(await row.textContent()).toContain("12.4k a week");
    await page.getByRole("button", { name: "Skill", exact: true }).click();
    await found().filter({ hasText: "@acme/pi-notes" }).waitFor();
    await expect.poll(() => found().count()).toBe(1);
    await page.getByRole("button", { name: "All", exact: true }).click();

    await row.getByRole("button", { name: "Add pi-web-access" }).click();
    await told().getByText("It is named for Reviewer.").waitFor();
    expect(requests("POST", "/v1/extensions/packages")[0]?.body).toEqual({
      source: "npm:pi-web-access",
      agent: "Reviewer",
    });
    await told().getByRole("link", { name: "See it in the Inbox" }).click();
    await page.waitForURL(/\/inbox\?change=41$/);
    const item = page.getByTestId("inbox-item");
    await item.waitFor();
    expect(await item.textContent()).toContain("Install pi-web-access");
  });

  it("shows an update waiting, with its changelog", async () => {
    await open("/agents/Coder?tab=extensions");
    fake().waitForUpdate("pi-notes", "0.3.0", "0.4.0");
    const row = installed().filter({ hasText: "pi-notes" });
    await row.getByRole("button", { name: "Update pi-notes" }).click();
    await page
      .getByRole("region", { name: "Changelog of pi-notes" })
      .getByText("Notes survive a restart.")
      .waitFor();
  });

  it("proposes removing a package without asking twice, since a person merges it", async () => {
    await open("/agents/Coder?tab=extensions");
    await page.getByRole("button", { name: "Remove pi-notes" }).click();
    await told().getByText("Proposed removing pi-notes 0.3.0.", { exact: false }).waitFor();
    expect(requests("DELETE", "/v1/extensions/packages/pi-notes")).toHaveLength(1);
  });
});

/** The desktop app's window has no system frame: its buttons are drawn in the page, at the top right. */
describe.skipIf(usesRealServer)("a sheet at the right edge and the window's buttons", () => {
  it("starts below the page header, so the window's buttons are not covered", async () => {
    server = await startServer();
    server.pagesB?.use();
    context = await browser.newContext({ viewport: { width: 1280, height: 720 } });
    await context.addInitScript(() => {
      const nothing = async () => {};
      Object.defineProperty(window, "moebiusDesktop", {
        value: {
          minimize: () => {},
          toggleMaximize: () => {},
          close: () => {},
          servers: async () => [],
          switchServer: nothing,
          removeServer: nothing,
          renameServer: nothing,
          connect: nothing,
          connectSsh: nothing,
        },
      });
    });
    page = await context.newPage();
    await page.goto(server.loginUrl);
    await page.getByRole("navigation", { name: "Pages", exact: true }).waitFor();
    await page.goto(`${server.url}/plugins`);
    await page.getByRole("button", { name: "Browse packages" }).first().click();
    await sheet().waitFor();
    const sheetBox = await sheet().boundingBox();
    const controls = await page.getByTestId("window-controls").boundingBox();
    if (!sheetBox || !controls) throw new Error("the sheet or the window's buttons drew no box");
    // The sheet begins under the row of the window's buttons, at its right edge.
    expect(sheetBox.y).toBeGreaterThanOrEqual(controls.y + controls.height);
    expect(sheetBox.x + sheetBox.width).toBeGreaterThan(controls.x);
    // Each of the three buttons and the sheet's own close are reachable at their own point.
    for (const name of ["Minimize", "Maximize", "Close"]) {
      const button = await page.getByRole("button", { name, exact: true }).boundingBox();
      if (!button) throw new Error(`${name} drew no box`);
      const top = await page.evaluate(
        ([x, y]) =>
          document
            .elementFromPoint(x as number, y as number)
            ?.closest("button")
            ?.getAttribute("aria-label"),
        [button.x + button.width / 2, button.y + button.height / 2],
      );
      expect(top).toBe(name);
    }
  });
});
