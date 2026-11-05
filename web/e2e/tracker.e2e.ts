/**
 * The issue tracker on the board in a real browser: sub-issues as stacks in their parent's column (added from a card's
 * menu, opened in place, counted, moved to another parent), the dock of an issue (its parent, blockers, labels, claim,
 * state and comments), the filter by label, and what the server refuses (a cycle, a second claim) said beside the
 * control that asked. The tests make their issues through the page, so they run against the fake server and a real one;
 * only what needs the fake's data or its way of answering is skipped for a real server. Every wait is on something the
 * page shows, never on time.
 */
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type Browser, type BrowserContext, chromium, type Locator, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { pickOption } from "./controls";
import { startServer, type TestServer, usesRealServer } from "./server";

let browser: Browser;
let server: TestServer;
let context: BrowserContext;
let page: Page;
const directories: string[] = [];

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
  for (const directory of directories.splice(0))
    rmSync(directory, { recursive: true, force: true });
});

/** A board to work on: the fake's, or a new repository on a real server. */
async function openBoard(): Promise<void> {
  server = await startServer();
  context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  if (!usesRealServer) {
    await page.goto(`${server.url}/board/1`);
  } else {
    const directory = mkdtempSync(join(tmpdir(), "moebius-e2e-repo-"));
    directories.push(directory);
    const response = await context.request.post(`${server.url}/v1/repositories`, {
      headers: { "X-Moebius": "1" },
      data: { name: "demo", url: directory },
    });
    expect(response.ok(), await response.text()).toBe(true);
    const made = (await response.json()) as { id: number };
    await page.goto(`${server.url}/board/${made.id}`);
  }
  await page.getByRole("heading", { name: "demo", level: 1 }).waitFor();
}

const dock = () => page.getByRole("region", { name: "Card inspector" });
const alert = (text: string | RegExp) => page.getByRole("alert").filter({ hasText: text });
const status = (text: string | RegExp) => page.getByRole("status").filter({ hasText: text });
/** The card of the issue with this title: its name is its number and its title. */
const cardTitled = (title: string) =>
  page.getByRole("group", { name: new RegExp(`^#\\d+ ${title}$`) });
const stackOf = (title: string) => page.getByTestId(/^stack-/).filter({ has: cardTitled(title) });
const menuOf = (where: Locator) => where.getByRole("button", { name: /^More actions for #/ });
const item = (name: string) => page.getByRole("menuitem", { name, exact: true });

/** The number of the issue of a card, read from its name. */
async function numberOf(title: string): Promise<number> {
  const name = (await cardTitled(title).first().getAttribute("aria-label")) ?? "";
  return Number(/^#(\d+)/.exec(name)?.[1]);
}

/** Makes an issue with the "New issue" button, and waits for the page to say it did. */
async function newIssue(title: string) {
  await page.getByRole("button", { name: "New issue" }).click();
  const form = page.getByRole("dialog", { name: "New issue" });
  await form.getByLabel("Title").fill(title);
  await form.getByRole("button", { name: "Create", exact: true }).click();
  await status(`Created "${title}"`).waitFor();
  await cardTitled(title).first().waitFor();
}

/** Makes a sub-issue from the menu of the card titled `parent`. */
async function addSubIssue(parent: string, title: string) {
  const number = await numberOf(parent);
  // On a real server the board moves a new card on while the menu opens, which redraws the card and shuts the menu:
  // the menu is opened again until the choice is made.
  await expect
    .poll(
      async () => {
        try {
          await menuOf(cardTitled(parent).first()).click({ timeout: 2000 });
          await item("Add a sub-issue…").click({ timeout: 2000 });
          return true;
        } catch {
          await page.keyboard.press("Escape");
          return false;
        }
      },
      { timeout: 30_000 },
    )
    .toBe(true);
  const form = page.getByRole("dialog", { name: "New issue" });
  // The form offers the parent it was opened for.
  expect(await form.getByLabel("Parent").textContent()).toContain(`#${number} ${parent}`);
  await form.getByLabel("Title").fill(title);
  await form.getByRole("button", { name: "Create", exact: true }).click();
  await status(`Created "${title}"`).waitFor();
}

/** The control that opens or shuts the stack of the card titled `title`. */
const toggleOf = (title: string) =>
  cardTitled(title)
    .first()
    .getByRole("button", { name: /^Sub-issues of #/ });

describe("sub-issues on the board", () => {
  it("lays sub-issues as a stack under their parent, counts the done ones and opens the stack in place", async () => {
    await openBoard();
    await newIssue("Plan the export");
    await addSubIssue("Plan the export", "Pick the format");
    await addSubIssue("Plan the export", "Write the exporter");

    // The parent is the card on the board; its sub-issues are under it, once the stack is opened.
    const toggle = toggleOf("Plan the export");
    await toggle.waitFor();
    expect(await toggle.textContent()).toBe("0 of 2 sub-issues done");
    // The dock opens on a card of its own choosing, and the stack of the card it shows is open; any other starts shut.
    if ((await toggle.getAttribute("aria-expanded")) === "false") {
      expect(await cardTitled("Pick the format").count()).toBe(0);
      await toggle.click();
    }
    await stackOf("Plan the export")
      .getByRole("group", { name: /Pick the format$/ })
      .waitFor();
    await stackOf("Plan the export")
      .getByRole("group", { name: /Write the exporter$/ })
      .waitFor();
    expect(await toggle.getAttribute("aria-expanded")).toBe("true");

    // A sub-issue is a card of its own: it opens, and its dock names its parent.
    await cardTitled("Pick the format").click();
    const parent = await numberOf("Plan the export");
    await dock().getByRole("heading", { name: "Pick the format" }).waitFor();
    await dock()
      .getByRole("button", { name: `#${parent} Plan the export` })
      .first()
      .waitFor();

    // Closing it counts it done, and the card goes to Done.
    await dock().getByRole("button", { name: "Close issue" }).click();
    await expect.poll(async () => await toggle.textContent()).toBe("1 of 2 sub-issues done");
    await dock().getByRole("button", { name: "Reopen issue" }).waitFor();
    await dock().getByRole("button", { name: "Reopen issue" }).click();
    await expect.poll(async () => await toggle.textContent()).toBe("0 of 2 sub-issues done");

    // Shutting the stack hides them again, unless the dock is showing one of them.
    await toggle.click();
    expect(await cardTitled("Write the exporter").count()).toBe(1);
    await dock().getByRole("button", { name: "Close the inspector" }).click();
    await toggle.click();
    await toggle.click();
    await cardTitled("Write the exporter").waitFor({ state: "detached" });
  });

  it("moves a sub-issue to another parent from the card's menu", async () => {
    await openBoard();
    await newIssue("First plan");
    await newIssue("Second plan");
    await addSubIssue("First plan", "A step");
    await toggleOf("First plan").click();
    await cardTitled("A step").waitFor();

    const second = await numberOf("Second plan");
    await menuOf(cardTitled("A step")).click();
    await item("Move to another parent…").click();
    const dialog = page.getByRole("dialog", { name: /^Move #\d+ to another parent$/ });
    await pickOption(page, dialog.getByLabel("Parent"), `#${second} Second plan`);
    await dialog.getByRole("button", { name: "Move", exact: true }).click();
    await status(new RegExp(`is a sub-issue of #${second}\\.`)).waitFor();

    // The stack of the first plan is gone with its only sub-issue, and the second has one now.
    await expect.poll(async () => await toggleOf("First plan").count()).toBe(0);
    await expect
      .poll(async () => await toggleOf("Second plan").textContent())
      .toBe("0 of 1 sub-issue done");

    // And it can be made an issue of its own again.
    await toggleOf("Second plan").click();
    await menuOf(cardTitled("A step")).click();
    await item("Move to another parent…").click();
    await pickOption(page, dialog.getByLabel("Parent"), "No parent");
    await dialog.getByRole("button", { name: "Move", exact: true }).click();
    await status(/is not a sub-issue any more\./).waitFor();
    await expect.poll(async () => await toggleOf("Second plan").count()).toBe(0);
  });

  it.skipIf(usesRealServer)(
    "shows an outsider's issue that names a parent as a card of its own until a person approves it",
    async () => {
      await openBoard();
      const outsider = "Fix the login redirect";
      const parent = "Document the API";
      const parentNumber = await numberOf(parent);
      await menuOf(cardTitled(outsider)).click();
      await item("Move to another parent…").click();
      const dialog = page.getByRole("dialog", { name: /^Move #\d+ to another parent$/ });
      await pickOption(page, dialog.getByLabel("Parent"), `#${parentNumber} ${parent}`);
      await dialog.getByRole("button", { name: "Move", exact: true }).click();
      await status(/is a sub-issue of #\d+\./).waitFor();

      // The server says it does not count, so it is no stack: the parent has none and the issue is on the board.
      await cardTitled(outsider).waitFor();
      expect(await toggleOf(parent).count()).toBe(0);
      expect(await stackOf(parent).count()).toBe(0);

      // A person approves it, and it is a sub-issue.
      const approved = server.board?.handle("POST", "/v1/cards/2/approve", {});
      expect(approved?.status).toBe(200);
      await expect
        .poll(async () => await toggleOf(parent).textContent())
        .toBe("0 of 1 sub-issue done");
    },
  );

  it("offers the parent in the form of a new issue", async () => {
    await openBoard();
    await newIssue("The plan");
    const number = await numberOf("The plan");
    await page.getByRole("button", { name: "New issue" }).click();
    const form = page.getByRole("dialog", { name: "New issue" });
    expect(await form.getByLabel("Parent").textContent()).toContain("No parent");
    await pickOption(page, form.getByLabel("Parent"), `#${number} The plan`);
    await form.getByLabel("Title").fill("Chosen as a sub-issue");
    await form.getByRole("button", { name: "Create", exact: true }).click();
    await status('Created "Chosen as a sub-issue"').waitFor();
    await stackOf("The plan").waitFor();
    expect(await toggleOf("The plan").textContent()).toBe("0 of 1 sub-issue done");
  });
});

describe("the issue in the dock", () => {
  it("claims and releases, labels, comments and edits an issue, and the card shows what it is given", async () => {
    await openBoard();
    await newIssue("Decide the queue");
    await cardTitled("Decide the queue").click();
    await dock().getByRole("heading", { name: "Decide the queue" }).waitFor();

    await dock().getByRole("button", { name: "Claim" }).click();
    await cardTitled("Decide the queue").getByText("Claimed by person").waitFor();
    await dock().getByText("Claimed by person").waitFor();
    await dock().getByRole("button", { name: "Release" }).click();
    await cardTitled("Decide the queue")
      .getByText("Claimed by person")
      .waitFor({ state: "detached" });

    await dock().getByLabel("New label").fill("wayfinder:decision");
    await dock().getByRole("button", { name: "Add label" }).click();
    await cardTitled("Decide the queue").getByText("wayfinder:decision").waitFor();
    await dock().getByRole("button", { name: "Remove label wayfinder:decision" }).waitFor();

    await dock().getByLabel("Write a comment").fill("A queue keeps the order.");
    await dock().getByRole("button", { name: "Comment", exact: true }).click();
    await dock().getByText("A queue keeps the order.").waitFor();
    await expect.poll(() => dock().getByLabel("Write a comment").inputValue()).toBe("");

    await dock().getByRole("button", { name: "Edit issue" }).click();
    const form = page.getByRole("dialog", { name: /^Edit #\d+$/ });
    await form.getByLabel("Title").fill("Decide the queue type");
    await form.getByLabel("Description").fill("FIFO or priority.");
    await form.getByRole("button", { name: "Save", exact: true }).click();
    await dock().getByRole("heading", { name: "Decide the queue type" }).waitFor();
    await dock().getByText("FIFO or priority.").waitFor();
  });

  it("filters the board by label, sub-issues included", async () => {
    await openBoard();
    await newIssue("Plan");
    await addSubIssue("Plan", "Spike it");
    await addSubIssue("Plan", "Write it");
    await newIssue("Unrelated");
    await toggleOf("Plan").click();
    await cardTitled("Spike it").click();
    await dock().getByRole("heading", { name: "Spike it" }).waitFor();
    await dock().getByLabel("New label").fill("spike");
    await dock().getByRole("button", { name: "Add label" }).click();
    await cardTitled("Spike it").getByText("spike", { exact: true }).waitFor();

    const filters = page.getByRole("search", { name: "Filter the cards" });
    await pickOption(page, filters.getByLabel("Label"), "spike");
    // The plan stays for the sub-issue that passes; the one that does not is hidden, and so is the other card.
    await cardTitled("Spike it").waitFor();
    await cardTitled("Write it").waitFor({ state: "detached" });
    await cardTitled("Unrelated").waitFor({ state: "detached" });
    expect(await toggleOf("Plan").textContent()).toBe("0 of 2 sub-issues done");
    await filters.getByRole("button", { name: "Clear filters" }).click();
    await cardTitled("Unrelated").waitFor();
    await cardTitled("Write it").waitFor();
  });

  it("refuses a blocker that makes a cycle beside the control that asked, and a blocker that is not an issue", async () => {
    await openBoard();
    await newIssue("Ship");
    await newIssue("Build");
    const ship = await numberOf("Ship");
    const build = await numberOf("Build");
    await cardTitled("Build").click();
    // The address names the card before the dock shows it: typed into the dock of the card it showed before, the number
    // would go with that dock.
    await dock().getByRole("heading", { name: "Build" }).waitFor();
    await dock().getByLabel("Blocked by issue number").fill(String(ship));
    await dock().getByRole("button", { name: "Add blocker" }).click();
    await dock()
      .getByRole("button", { name: `Remove blocker #${ship}` })
      .waitFor();
    // The card says it waits, as it does for an issue that blocks it any other way.
    await cardTitled("Build").getByText(`Waiting on #${ship}`).waitFor();

    await cardTitled("Ship").click();
    await dock().getByRole("heading", { name: "Ship" }).waitFor();
    await dock().getByLabel("Blocked by issue number").fill(String(build));
    await dock().getByRole("button", { name: "Add blocker" }).click();
    await alert("cannot block it").waitFor();
    await dock().getByLabel("Blocked by issue number").fill("9999");
    await dock().getByRole("button", { name: "Add blocker" }).click();
    await alert("not found").waitFor();

    // Closing the blocker frees the card.
    await dock().getByRole("button", { name: "Close issue" }).click();
    await cardTitled("Build").getByText(`Waiting on #${ship}`).waitFor({ state: "detached" });
  });
});
