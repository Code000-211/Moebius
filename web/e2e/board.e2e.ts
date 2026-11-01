/**
 * The board page in a real browser: its columns and cards, cards that move when the server moves them, the decisions
 * (approve, merge, send back, retry), dragging only where a person may move a card and the reason when a drop is
 * refused, the dock (a long title, the history, the runs, the script), the filters and a new issue. The fake server
 * (`fake/board.ts`) has the data; against a real server (MOEBIUS_E2E_SERVER) the tests that need that data are skipped
 * and the board of a new repository is used. Every wait is on something the page shows, never on time.
 */
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type Browser, type BrowserContext, chromium, type Locator, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { LONG_TITLE } from "./fake/board";
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

/** A new server and a normal browser (the columns are wider than it, so some start out of view), signed in. */
async function fresh() {
  server = await startServer();
  context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
}

afterEach(async () => {
  await context.close();
  await server.stop().catch(() => {});
  for (const directory of directories.splice(0))
    rmSync(directory, { recursive: true, force: true });
});

/** The fake's board; a real server has none, so a repository is made (a new folder becomes a repository). */
async function openBoard(): Promise<void> {
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

const fake = () => {
  if (!server.board) throw new Error("this test needs the fake server");
  return server.board;
};
/** Sends an event to the page, as the fake server's event stream. */
const broadcast = (event: { kind: string; data: unknown }) => {
  if (!server.broadcast) throw new Error("this test needs the fake server");
  server.broadcast(event);
};
const column = (name: string) => page.getByRole("region", { name, exact: true });
const card = (number: number) => page.getByTestId(`card-${number}`);
const dock = () => page.getByRole("region", { name: "Card inspector" });
const alert = (text: string | RegExp) => page.getByRole("alert").filter({ hasText: text });
const status = (text: string | RegExp) => page.getByRole("status").filter({ hasText: text });

/** What the screen reader is told last (the live region of the drag library). */
const spoken = async () => (await page.locator("[id^=DndLiveRegion]").textContent()) ?? "";

/** Lifts a card with the keyboard, moves it `steps` columns to the right (negative: left) and drops it. Every key waits for the page to have taken the one before, as a person's keys do. */
async function drag(target: Locator, steps: number) {
  await target.focus();
  await page.keyboard.press("Space");
  await expect.poll(spoken).toContain("is over the column");
  // The library listens for the arrow keys one task after the lift: let that task run (not a wait for time).
  await page.evaluate(() => new Promise((resolve) => setTimeout(resolve, 0)));
  const key = steps > 0 ? "ArrowRight" : "ArrowLeft";
  for (let i = 0; i < Math.abs(steps); i++) {
    const before = await spoken();
    await page.keyboard.press(key);
    await expect.poll(spoken).not.toBe(before);
  }
  await page.keyboard.press("Space");
}

describe("the board page", () => {
  it("loads for a real server and for the fake, with the columns of the default workflow", async () => {
    await fresh();
    await openBoard();
    for (const name of ["Ready", "In progress", "In review", "Ready to merge", "Done"]) {
      await column(name).waitFor();
    }
    await page.getByRole("button", { name: "New issue" }).waitFor();
    await page.getByRole("search", { name: "Filter the cards" }).waitFor();
  });

  it("makes a card from a new issue", async () => {
    await fresh();
    await openBoard();
    await page.getByRole("button", { name: "New issue" }).click();
    const create = page.getByRole("button", { name: "Create", exact: true });
    expect(await create.isDisabled()).toBe(true);
    await page.getByRole("dialog").getByLabel("Title").fill("Write the changelog");
    await page.getByRole("dialog").getByLabel("Description").fill("List what changed.");
    await create.click();
    await status("Created issue #").waitFor();
    // The card is on the board: in the Backlog, unless the board has moved it on already.
    await page.getByText("Write the changelog").first().waitFor();
    if (!usesRealServer) {
      await column("Backlog").getByText("Write the changelog").waitFor();
      expect(fake().requests.at(-1)).toMatchObject({
        method: "POST",
        path: "/v1/repositories/1/issues",
        body: { title: "Write the changelog", body: "List what changed." },
      });
    }
  });

  it.skipIf(usesRealServer)("shows the server's refusal in the form of a new issue", async () => {
    await fresh();
    await openBoard();
    fake().refuseNext("POST", "/v1/repositories/1/issues", 500, "the forge refused the issue");
    await page.getByRole("button", { name: "New issue" }).click();
    await page.getByRole("dialog").getByLabel("Title").fill("Doomed");
    await page.getByRole("button", { name: "Create", exact: true }).click();
    await alert("the forge refused the issue").waitFor();
    // The form stays open with what was typed.
    expect(await page.getByRole("dialog").getByLabel("Title").inputValue()).toBe("Doomed");
  });

  it.skipIf(usesRealServer)(
    "shows each card with its issue, status, agent, attempts, change and pull request",
    async () => {
      await fresh();
      await openBoard();
      // Columns: Backlog is there because a card is in it; the counts follow the cards.
      await column("Backlog").waitFor();
      expect(
        await column("Ready")
          .getByTestId(/^card-/)
          .count(),
      ).toBe(2);

      const running = card(1);
      await running.waitFor();
      expect(
        await running.getByRole("link", { name: "Issue #1 on its forge" }).getAttribute("href"),
      ).toBe("https://github.com/acme/demo/issues/1");
      await running.getByRole("img", { name: "Running" }).waitFor();
      await running.getByText("Live", { exact: true }).waitFor();
      await running.getByText("Coder · round 1 of 5").waitFor();
      await running.getByRole("img", { name: "Round 1 of 5" }).waitFor();

      const ready = card(5);
      const pull = ready.getByRole("link", { name: "Pull request #11, open" });
      expect(await pull.getAttribute("href")).toBe("https://github.com/acme/demo/pull/11");
      await ready.getByText("Green").waitFor();
      await ready.getByText("Waits for you").waitFor();
      await ready.getByText("2 rounds · 96k tok").waitFor();

      await card(4).getByText("Blocked by #3").waitFor();
      await card(3).getByText("Next").waitFor();
      await card(6).getByText("Stuck").waitFor();
      await card(2).getByText("Waits for you").waitFor();
      await card(7).getByRole("link", { name: "Pull request #9, merged" }).waitFor();
    },
  );
});

describe("live data", () => {
  it.skipIf(usesRealServer)("moves a card when the server moves it", async () => {
    await fresh();
    await openBoard();
    await column("Ready").getByTestId("card-3").waitFor();
    broadcast(fake().moveCard(3, "In progress"));
    await column("In progress").getByTestId("card-3").waitFor();
    expect(await column("Ready").getByTestId("card-3").count()).toBe(0);
  });

  it.skipIf(usesRealServer)("shows a card that the server adds", async () => {
    await fresh();
    await openBoard();
    // Another client makes the issue; this page learns of it from the event.
    const made = await context.request.post(`${server.url}/v1/repositories/1/issues`, {
      headers: { "X-Moebius": "1" },
      data: { title: "Made elsewhere" },
    });
    expect(made.ok()).toBe(true);
    await column("Backlog").getByText("Made elsewhere").waitFor();
  });

  it.skipIf(usesRealServer)("shows the new state of the inspected card as it moves", async () => {
    await fresh();
    await openBoard();
    await page.goto(`${server.url}/board/1?card=3`);
    await dock().getByText("demo · Ready · by person").waitFor();
    broadcast(fake().moveCard(3, "In review"));
    await dock().getByText("demo · In review · by person").waitFor();
  });
});

describe("the decisions", () => {
  it.skipIf(usesRealServer)(
    "approves an issue from the dock and moves the card to Ready",
    async () => {
      await fresh();
      await openBoard();
      await card(2).click();
      await dock().getByText("Approve: Fix the login redirect").waitFor();
      await dock().getByRole("button", { name: "Approve" }).click();
      await status("Approved #2").waitFor();
      await column("Ready").getByTestId("card-2").waitFor();
      expect(fake().requests.at(-1)).toMatchObject({ method: "POST", path: "/v1/cards/2/approve" });
      // Nothing waits for you any more.
      await dock().getByRole("button", { name: "Approve" }).waitFor({ state: "detached" });
    },
  );

  it.skipIf(usesRealServer)(
    "merges a change from the dock and moves the card to Done",
    async () => {
      await fresh();
      await openBoard();
      await card(5).click();
      await dock().getByRole("button", { name: "Merge" }).click();
      await status("Merged #5. Commit 5ca1ab1.").waitFor();
      await column("Done").getByTestId("card-5").waitFor();
      expect(fake().requests.at(-1)).toMatchObject({ method: "POST", path: "/v1/changes/1/merge" });
    },
  );

  it.skipIf(usesRealServer)("sends a change back with a reason", async () => {
    await fresh();
    await openBoard();
    await card(5).click();
    await dock().getByRole("button", { name: "Reject with a reason" }).click();
    const send = dock().getByRole("button", { name: "Send back" });
    expect(await send.isDisabled()).toBe(true);
    await dock().getByLabel("Why? The next round is told").fill("The tests are missing");
    await send.click();
    await status("Sent #5 back to coding.").waitFor();
    await column("In progress").getByTestId("card-5").waitFor();
    expect(fake().requests.at(-1)).toMatchObject({
      method: "POST",
      path: "/v1/changes/1/reject",
      body: { reason: "The tests are missing" },
    });
    await dock().getByText("The tests are missing").first().waitFor();
  });

  it.skipIf(usesRealServer)("retries a stuck card", async () => {
    await fresh();
    await openBoard();
    await card(6).click();
    await dock().getByRole("button", { name: "Retry" }).click();
    await status("Retrying #6.").waitFor();
    expect(fake().requests.at(-1)).toMatchObject({ method: "POST", path: "/v1/cards/6/retry" });
    await card(6).getByText("Live", { exact: true }).waitFor();
  });

  it.skipIf(usesRealServer)("tells why the server refused a decision", async () => {
    await fresh();
    await openBoard();
    fake().refuseNext("POST", "/v1/changes/1/merge", 409, "the branch does not merge cleanly");
    await card(5).click();
    await dock().getByRole("button", { name: "Merge" }).click();
    await alert("Could not merge #5: the branch does not merge cleanly").waitFor();
    // The card did not move, and the buttons are there to try again.
    await column("Ready to merge").getByTestId("card-5").waitFor();
    await dock().getByRole("button", { name: "Merge" }).waitFor();
  });
});

describe("dragging cards", () => {
  it.skipIf(usesRealServer)(
    "refuses a drop where a person may not move the card, with the reason",
    async () => {
      await fresh();
      await openBoard();
      const before = fake().requests.length;
      // Ready -> Done: four columns to the right.
      await drag(card(3), 4);
      await alert("Cannot move #3 to Done: Nobody decides this card now").waitFor();
      await column("Ready").getByTestId("card-3").waitFor();
      expect(fake().requests.length).toBe(before);
    },
  );

  it.skipIf(usesRealServer)("refuses to move a card that is working, saying so", async () => {
    await fresh();
    await openBoard();
    await drag(card(1), 3);
    await alert("Moebius is working on this card").waitFor();
    await column("In progress").getByTestId("card-1").waitFor();
  });

  it.skipIf(usesRealServer)("approves an issue dropped on Ready", async () => {
    await fresh();
    await openBoard();
    await drag(card(2), 1);
    await status("Approved #2").waitFor();
    await column("Ready").getByTestId("card-2").waitFor();
    expect(fake().requests.at(-1)).toMatchObject({ method: "POST", path: "/v1/cards/2/approve" });
  });

  it.skipIf(usesRealServer)(
    "keyboard-drags a card to a column that starts out of view and scrolls it into view",
    async () => {
      await fresh();
      await openBoard();
      const last = column("Done");
      const inView = () =>
        last.evaluate((node) => {
          const box = node.getBoundingClientRect();
          const scroller = node.parentElement?.parentElement?.getBoundingClientRect();
          return !!scroller && box.right <= scroller.right + 1 && box.left >= scroller.left - 1;
        });
      expect(await inView()).toBe(false);
      await drag(card(5), 1);
      await status("Merged #5").waitFor();
      expect(await inView()).toBe(true);
    },
  );

  it.skipIf(usesRealServer)("announces a keyboard drag in words", async () => {
    await fresh();
    await openBoard();
    await card(2).focus();
    await page.keyboard.press("Space");
    // The announcement of the lift is replaced at once by the one of the column under the card.
    const live = page.locator("[id^=DndLiveRegion]");
    await live
      .filter({ hasText: "#2 Fix the login redirect is over the column Backlog" })
      .waitFor({ state: "attached" });
    // The library listens for the arrow keys one task after the lift (as in drag() above).
    await page.evaluate(() => new Promise((resolve) => setTimeout(resolve, 0)));
    await page.keyboard.press("ArrowRight");
    await expect.poll(spoken).toContain("is over the column Ready");
    await page.keyboard.press("Escape");
    await live.filter({ hasText: "Cancelled. #2" }).waitFor({ state: "attached" });
  });

  it.skipIf(usesRealServer)("drags a card with the mouse", async () => {
    await fresh();
    await openBoard();
    const from = (await card(2).boundingBox()) as {
      x: number;
      y: number;
      width: number;
      height: number;
    };
    const to = (await column("Ready").boundingBox()) as {
      x: number;
      y: number;
      width: number;
      height: number;
    };
    await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
    await page.mouse.down();
    // A few steps, so that the drag starts and the card is over the column before it is let go.
    await page.mouse.move(from.x + from.width / 2 + 20, from.y + 20, { steps: 5 });
    await page.mouse.move(to.x + to.width / 2, to.y + 120, { steps: 20 });
    await page.mouse.up();
    await status("Approved #2").waitFor();
    await column("Ready").getByTestId("card-2").waitFor();
  });

  it.skipIf(usesRealServer)(
    "refuses an issue that waits for approval dropped anywhere but Ready",
    async () => {
      await fresh();
      await openBoard();
      await drag(card(2), 2);
      await alert("This issue waits for your approval: drop it on Ready").waitFor();
      await column("Backlog").getByTestId("card-2").waitFor();
    },
  );

  it.skipIf(usesRealServer)("merges a change dropped on Done", async () => {
    await fresh();
    await openBoard();
    await drag(card(5), 1);
    await status("Merged #5").waitFor();
    await column("Done").getByTestId("card-5").waitFor();
    expect(fake().requests.at(-1)).toMatchObject({ method: "POST", path: "/v1/changes/1/merge" });
  });

  it.skipIf(usesRealServer)(
    "asks for a reason when a change is dropped on In progress",
    async () => {
      await fresh();
      await openBoard();
      const before = fake().requests.length;
      await drag(card(5), -2);
      await dock().getByLabel("Why? The next round is told").waitFor();
      expect(fake().requests.length).toBe(before);
      await dock().getByLabel("Why? The next round is told").fill("Needs a test");
      await dock().getByRole("button", { name: "Send back" }).click();
      await column("In progress").getByTestId("card-5").waitFor();
    },
  );

  it.skipIf(usesRealServer)("retries a stuck card dropped on In progress", async () => {
    await fresh();
    await openBoard();
    // The stuck card is in In progress already: the drop on its own column retries it.
    await drag(card(6), 0);
    await status("Retrying #6.").waitFor();
  });
});

describe("the dock", () => {
  it.skipIf(usesRealServer)(
    "shows the history, the runs and the script of the column of the card",
    async () => {
      await fresh();
      await openBoard();
      // Without a choice the dock shows the first live card.
      await dock().getByRole("heading", { name: "Add a feature" }).waitFor();
      await dock().getByText("Moved from Backlog to Ready", { exact: true }).waitFor();
      await dock().getByText("Run 1 started: Coder, code").waitFor();
      const run = dock().getByRole("link", { name: "Run 1", exact: true });
      expect(await run.getAttribute("href")).toBe("/runs/1");
      await dock().getByText(".moebius/code.luau · built in").waitFor();
      await dock().getByText('moebius.wait("nothing to do in In progress")').waitFor();
      await dock().getByText("Coder · code (running)").waitFor();

      await dock().getByRole("link", { name: "Open the script" }).click();
      await page.waitForURL("**/board/1/scripts/In%20progress");
      await page.getByRole("heading", { name: "Script of In progress" }).waitFor();
      await page.getByText("built in").waitFor();
      await page.getByText("-- The In progress column").waitFor();
    },
  );

  it.skipIf(usesRealServer)("links a run to its page", async () => {
    await fresh();
    await openBoard();
    await dock().getByRole("link", { name: "Run 1", exact: true }).click();
    await page.waitForURL("**/runs/1");
    await page.getByRole("heading", { name: "Run 1" }).waitFor();
  });

  it.skipIf(usesRealServer)("wraps a long title inside the dock and the card", async () => {
    await fresh();
    await openBoard();
    await card(4).click();
    const title = dock().getByRole("heading", { name: LONG_TITLE });
    await title.waitFor();
    const panel = (await dock().boundingBox()) as { x: number; width: number };
    const heading = (await title.boundingBox()) as { x: number; width: number; height: number };
    expect(heading.x + heading.width).toBeLessThanOrEqual(panel.x + panel.width);
    // It takes several lines, and nothing in the dock scrolls sideways.
    expect(heading.height).toBeGreaterThan(40);
    const scrolls = await dock().evaluate((element) =>
      [element, ...element.querySelectorAll("*")].some(
        (node) =>
          node.scrollWidth > node.clientWidth && getComputedStyle(node).overflowX === "visible",
      ),
    );
    expect(scrolls).toBe(false);
    // The card keeps to its column, too.
    const lane = (await column("Ready").boundingBox()) as { x: number; width: number };
    const tile = (await card(4).boundingBox()) as { x: number; width: number };
    expect(tile.x + tile.width).toBeLessThanOrEqual(lane.x + lane.width);
  });

  it.skipIf(usesRealServer)("keeps the chosen card in the address, and closes", async () => {
    await fresh();
    await openBoard();
    await card(3).click();
    await page.waitForURL("**/board/1?card=3");
    await dock().getByRole("heading", { name: "Document the API" }).waitFor();
    await page.reload();
    await dock().getByRole("heading", { name: "Document the API" }).waitFor();
    await dock().getByRole("button", { name: "Close the inspector" }).click();
    await dock().waitFor({ state: "detached" });
    await page.reload();
    await column("Ready").waitFor();
    expect(await dock().count()).toBe(0);
  });
});

describe("the filters", () => {
  it.skipIf(usesRealServer)("filters by column, agent and status, and clears", async () => {
    await fresh();
    await openBoard();
    const filters = page.getByRole("search", { name: "Filter the cards" });
    const shown = () => page.getByTestId(/^card-/).count();
    expect(await shown()).toBe(7);

    await filters.getByLabel("Column").selectOption("Done");
    await card(7).waitFor();
    expect(await shown()).toBe(1);
    expect(await column("Ready").count()).toBe(0);
    await page.waitForURL("**column=Done");

    await filters.getByRole("button", { name: "Clear" }).click();
    await card(3).waitFor();
    expect(await shown()).toBe(7);

    await filters.getByLabel("Agent").selectOption("Reviewer");
    await card(6).waitFor();
    expect(await shown()).toBe(1);
    await filters.getByLabel("Agent").selectOption("");

    await filters.getByLabel("Status").selectOption("needs-you");
    await card(2).waitFor();
    expect(await shown()).toBe(2);
    await card(5).waitFor();

    await filters.getByLabel("Status").selectOption("blocked");
    await card(4).waitFor();
    expect(await shown()).toBe(1);

    // Two filters together, and a reload keeps them.
    await filters.getByLabel("Status").selectOption("stuck");
    await filters.getByLabel("Agent").selectOption("Coder");
    await page.reload();
    await column("Ready")
      .waitFor({ state: "detached" })
      .catch(() => {});
    expect(await shown()).toBe(0);
    expect(await filters.getByLabel("Agent").inputValue()).toBe("Coder");
  });

  it.skipIf(usesRealServer)(
    "lists the repositories and opens the board of the one chosen",
    async () => {
      await fresh();
      await openBoard();
      const filters = page.getByRole("search", { name: "Filter the cards" });
      expect(await filters.getByLabel("Repository").inputValue()).toBe("1");
      expect(await filters.getByLabel("Repository").locator("option").allTextContents()).toEqual([
        "demo",
      ]);
    },
  );

  it("says there is no such board for a repository that does not exist", async () => {
    await fresh();
    await page.goto(`${server.url}/board/99999`);
    await page.getByText("There is no such board").waitFor();
  });
});
