/**
 * The Milestones page in a real browser: the list with each milestone's progress, opened to its cards of two projects,
 * the timeline Display chooses, the filter by project, making, editing and deleting one, the numbers that leave out a
 * project the person may not see, and a card's Milestone property. They run against the fake server of
 * `fake/milestones.ts`; the rules of what a person may see are the server's own and tested there. Every wait is on
 * something the page shows, never on time.
 */
import { type Browser, type BrowserContext, chromium, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
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

async function open(path: string) {
  server = await startServer();
  context = await browser.newContext({ viewport: { width: 1400, height: 1000 } });
  page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  await page.goto(`${server.url}${path}`);
}

const fake = () => {
  if (!("milestones" in server)) throw new Error("this test needs the fake server");
  return server.milestones;
};
const rows = () => page.getByTestId("milestone");
const row = (name: string) => rows().filter({ hasText: name });
const bar = (name: string) => page.getByRole("progressbar", { name }).first();

describe.skipIf(usesRealServer)("the Milestones page", () => {
  it("is in the sidebar under Plan and lists the milestones by target date with their progress", async () => {
    await open("/inbox");
    await page
      .getByRole("navigation", { name: "Pages" })
      .getByRole("link", { name: "Milestones" })
      .click();
    await page.getByRole("heading", { name: "Milestones", level: 1 }).waitFor();
    await rows().first().waitFor();
    expect(await rows().count()).toBe(3);
    const names = await rows()
      .getByRole("button")
      .evaluateAll((buttons) =>
        buttons.map((button) => button.querySelector("span span")?.textContent),
      );
    expect(names).toEqual(["Launch 2.0", "Payments v2", "Annual report"]);
    // Launch 2.0 holds two cards of the board's project and two of Storefront; two are done.
    await row("Launch 2.0").getByText("2 projects").waitFor();
    await row("Launch 2.0").getByText("2 of 4 cards").waitFor();
    expect(await bar("2 of 4 cards").getAttribute("aria-valuenow")).toBe("50");
    await row("Payments v2").getByText("2 of 3 cards").waitFor();
    await row("Annual report").getByText("0 of 1 card").waitFor();
  });

  it("opens a milestone to its description, its share in each project and its cards of both", async () => {
    await open("/milestones");
    await row("Launch 2.0").getByRole("button").first().click();
    await row("Launch 2.0").getByText("Everything for the launch.").waitFor();
    await row("Launch 2.0").getByText("DEMO 0 of 2 cards").waitFor();
    await row("Launch 2.0").getByText("SHOP 2 of 2 cards").waitFor();
    const cards = row("Launch 2.0").getByRole("list", { name: "Cards of Launch 2.0" });
    await cards.getByText("Add a feature").waitFor();
    await cards.getByText("Take payments").waitFor();
    expect(await cards.getByRole("listitem").count()).toBe(4);
  });

  it("shows the milestones as a timeline in Display and keeps the choice in the address", async () => {
    await open("/milestones");
    await rows().first().waitFor();
    await page.getByRole("button", { name: "Display" }).click();
    await page.getByRole("menuitemradio", { name: "Timeline" }).click();
    const table = page.getByRole("table", { name: "Milestones on a timeline" });
    await table.waitFor();
    expect(new URL(page.url()).searchParams.get("layout")).toBe("timeline");
    expect(await rows().count()).toBe(0);
    expect(await table.getByRole("row").count()).toBe(4);
    await table.getByRole("rowheader", { name: /Launch 2.0/ }).waitFor();
    const launch = table.getByRole("button", {
      name: /^Launch 2.0: 2 of 4 cards, due 2030-12-12$/,
    });
    await launch.waitFor();
    // The bars follow the dates: the milestone due first starts no later and ends before the one due last.
    const box = async (name: RegExp) => {
      const found = await table.getByRole("button", { name }).boundingBox();
      if (!found) throw new Error(`no bar named ${name}`);
      return found;
    };
    const first = await box(/^Launch 2.0/);
    const last = await box(/^Annual report/);
    expect(first.x + first.width).toBeLessThan(last.x + last.width);
    // A reload keeps the timeline.
    await page.reload();
    await table.waitFor();
    // Back to the list.
    await page.getByRole("button", { name: "Display" }).click();
    await page.getByRole("menuitemradio", { name: "List" }).click();
    await rows().first().waitFor();
  });

  it("sorts by name and filters to the milestones that hold cards of a project", async () => {
    await open("/milestones");
    await rows().first().waitFor();
    await page.getByRole("button", { name: "Display" }).click();
    await page.getByRole("menuitemradio", { name: "Name" }).click();
    await page.keyboard.press("Escape");
    await expect.poll(async () => (await rows().first().textContent())?.slice(0, 6)).toBe("Annual");

    await page.getByRole("button", { name: "Filter", exact: true }).click();
    await page.getByRole("menuitem", { name: "Project", exact: true }).click();
    await page.getByRole("menuitemradio", { name: "demo", exact: true }).click();
    // Payments v2 holds only Storefront's cards.
    await row("Launch 2.0").waitFor();
    await row("Payments v2").waitFor({ state: "detached" });
    expect(await rows().count()).toBe(2);
  });

  it("makes, edits and deletes a milestone, and says what the server refuses", async () => {
    await open("/milestones");
    await rows().first().waitFor();
    await page.getByRole("button", { name: "New milestone" }).first().click();
    const dialog = page.getByRole("dialog", { name: "New milestone" });
    const make = dialog.getByRole("button", { name: "Make milestone" });
    expect(await make.isDisabled()).toBe(true);
    await dialog.getByLabel("Name").fill("launch 2.0");
    await dialog.getByLabel("Target date").fill("2031-03-31");
    await make.click();
    await dialog
      .getByRole("alert")
      .getByText(/a milestone named launch 2.0 exists already/)
      .waitFor();
    await dialog.getByLabel("Name").fill("Beta");
    await make.click();
    await dialog.waitFor({ state: "detached" });
    await row("Beta").getByText("No cards yet").waitFor();
    await row("Beta").getByText("0 of 0 cards").waitFor();
    expect(fake().requests.at(-1)).toMatchObject({
      method: "POST",
      path: "/v1/milestones",
      body: { name: "Beta", target_date: "2031-03-31" },
    });

    await row("Beta").getByRole("button").first().click();
    await row("Beta").getByRole("button", { name: "Edit" }).click();
    const edit = page.getByRole("dialog", { name: "Edit milestone" });
    await edit.getByLabel("Name").fill("Beta 2");
    await edit.getByRole("button", { name: "Save" }).click();
    await edit.waitFor({ state: "detached" });
    await row("Beta 2").waitFor();

    await row("Beta 2").getByRole("button", { name: "Delete" }).click();
    await page.getByRole("dialog").getByRole("button", { name: "Delete" }).click();
    await row("Beta 2").waitFor({ state: "detached" });
    expect(await rows().count()).toBe(3);
  });

  it("counts only the cards of the projects the person may see, and says so after the server changes", async () => {
    await open("/milestones");
    await row("Launch 2.0").getByText("2 of 4 cards").waitFor();
    fake().hide(100);
    await page.reload();
    await row("Launch 2.0").getByText("0 of 2 cards").waitFor();
    await row("Launch 2.0").getByText("Storefront").waitFor({ state: "detached" });
    // Payments v2 held only Storefront's cards: it is still listed, with none counted.
    await row("Payments v2").getByText("0 of 0 cards").waitFor();
    await row("Payments v2").getByText("No cards yet").waitFor();
  });

  it("shows a card's milestones in its properties and puts it in another", async () => {
    await open("/board/1");
    await page.getByRole("heading", { name: "demo", level: 1 }).waitFor();
    await page.getByRole("group", { name: /^#1 Add a feature$/ }).click();
    const properties = page.getByRole("region", { name: /^Properties of #/ });
    await properties.getByRole("button", { name: "Milestone Launch 2.0" }).waitFor();
    await properties.getByRole("button", { name: "Milestone Launch 2.0" }).click();
    await page.getByRole("menuitemcheckbox", { name: "Annual report" }).click();
    await properties
      .getByRole("button", { name: /^Milestone Launch 2.0\s*Annual report$/ })
      .waitFor();
    expect(fake().requests.at(-1)).toMatchObject({
      method: "PUT",
      path: "/v1/cards/1/milestones",
      body: { milestones: [1, 3] },
    });
    // The page of milestones counts it now.
    await page.goto(`${server.url}/milestones`);
    await row("Annual report").getByText("0 of 2 cards").waitFor();
  });
});
