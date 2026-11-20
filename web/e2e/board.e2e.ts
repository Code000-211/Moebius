/**
 * The board page in a real browser: its columns and cards, cards that move when the server moves them, the decisions
 * (approve, merge, send back, retry), dragging only where a person may move a card and the reason when a drop is
 * refused, the dock (a long title, the history, the runs, the script), the filters, a new issue, and blocking,
 * unblocking and deleting a card from the menu of a board card and of the dock. The fake server
 * (`fake/board.ts`) has the data; against a real server (MOEBIUS_E2E_SERVER) the tests that need that data are skipped
 * and the board of a new repository is used. Every wait is on something the page shows, never on time.
 */
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type Browser, type BrowserContext, chromium, type Locator, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { addFilter, offers, showing, unfold } from "./controls";
import { LONG_TITLE } from "./fake/board";
import { startServer, type TestServer, usesRealServer } from "./server";

let browser: Browser;
let server: TestServer;
let context: BrowserContext;
let page: Page;
const directories: string[] = [];
/** The board a real server was given in this test, opened again rather than made twice (its name is taken). */
let demo: number | null = null;

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
  demo = null;
  for (const directory of directories.splice(0))
    rmSync(directory, { recursive: true, force: true });
});

/** The fake's board; a real server has none, so a repository is made (a new folder becomes a repository). */
async function openBoard(): Promise<void> {
  if (!usesRealServer) {
    await page.goto(`${server.url}/board/1`);
  } else {
    if (demo === null) {
      const directory = mkdtempSync(join(tmpdir(), "moebius-e2e-repo-"));
      directories.push(directory);
      const response = await context.request.post(`${server.url}/v1/repositories`, {
        headers: { "X-Moebius": "1" },
        data: { name: "demo", url: directory },
      });
      expect(response.ok(), await response.text()).toBe(true);
      demo = ((await response.json()) as { id: number }).id;
    }
    await page.goto(`${server.url}/board/${demo}`);
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
// By the label, not the role: a card window over the board is modal and hides the board from the accessibility tree.
/** Opens the line for a new card in a column from its context menu: the column has no button for it. */
async function addFromMenu(name: string): Promise<void> {
  await column(name).getByRole("heading", { name }).click({ button: "right" });
  await page.getByRole("menuitem", { name: "Add a card" }).click();
}
const column = (name: string) => page.locator(`section[aria-label="${name}"]`);
const card = (number: number) => page.getByTestId(`card-${number}`);
const dock = () => page.getByRole("dialog", { name: "Card inspector" });
/** The properties of the card numbered `number`, which the dock puts in the right column. */
const properties = (number: number) =>
  page.getByRole("region", { name: `Properties of #${number}`, exact: true });
/** The properties of the card numbered `number`, with the fold "More properties" open, once they are drawn. */
const unfolded = async (number: number) => {
  const region = properties(number);
  await region.waitFor();
  await unfold(region);
  return region;
};
const alert = (text: string | RegExp) => page.getByRole("alert").filter({ hasText: text });
const status = (text: string | RegExp) => page.getByRole("status").filter({ hasText: text });

/** Moves the focus down a menu to `target`, one arrow at a time: each waits for the focus to have moved, as a person's keys do, so a key is never pressed twice for one step. */
async function arrowDownTo(target: Locator) {
  for (let step = 0; step < 12; step++) {
    if (await target.evaluate((el) => el === document.activeElement)) return;
    const before = await page.evaluate(() => document.activeElement?.textContent ?? "");
    await page.keyboard.press("ArrowDown");
    await page.waitForFunction(
      (was) => (document.activeElement?.textContent ?? "") !== was,
      before,
    );
  }
  throw new Error("the focus did not reach the row");
}

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

describe("a card named by its key", () => {
  it.skipIf(usesRealServer)(
    "is shown as a page at its address, and a forge's card shows its key beside its number",
    async () => {
      await fresh();
      await page.goto(`${server.url}/card/demo-31`);
      await page.getByText("DEMO-31 · #7").first().waitFor();
      expect(new URL(page.url()).pathname).toBe("/card/demo-31");
      expect(await dock().count()).toBe(0);
      await page.getByRole("button", { name: "Edit issue" }).first().waitFor();
      await page.goto(`${server.url}/card/DEMO-99`);
      await page.getByText("There is no such card").waitFor();
    },
  );
});

describe("the board page", () => {
  it("loads for a real server and for the fake, with the columns of the default workflow", async () => {
    await fresh();
    await openBoard();
    for (const name of ["Not started", "In progress", "In review", "Ready to merge", "Done"]) {
      await column(name).waitFor();
    }
    await page.getByRole("button", { name: "New issue" }).waitFor();
    await page.getByRole("search", { name: "Filter the cards" }).waitFor();
  });

  it("says on the board that a .moebius folder the repository still has is not read", async () => {
    // Only the fake server has a repository with one (see `fake/pages-a.ts`).
    if (usesRealServer) return;
    await fresh();
    await openBoard();
    const notice = page.getByRole("status").filter({ hasText: "still has a .moebius folder" });
    await notice.waitFor();
    expect(await notice.textContent()).toContain(".moebius/old.luau");
    await notice.getByRole("link", { name: "Project settings" }).waitFor();
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
    await status('Created "Write the changelog"').waitFor();
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

  it("makes a card in place in a column, and leaves the line on Escape", async () => {
    await fresh();
    await openBoard();
    const backlog = column("Backlog");
    await addFromMenu("Backlog");
    const line = backlog.getByLabel("Title of the new card in Backlog");
    await line.fill("Never made");
    await line.press("Escape");
    await line.waitFor({ state: "detached" });
    expect(await page.getByText("Never made").count()).toBe(0);

    // The keyboard reaches it too: Enter on the column's name opens the same menu.
    await backlog.getByRole("heading", { name: "Backlog" }).press("Enter");
    await page.getByRole("menuitem", { name: "Add a card" }).click();
    await line.fill("Made in place");
    await line.press("Enter");
    await status('Created "Made in place"').waitFor();
    await backlog.getByText("Made in place").waitFor();
    await line.waitFor({ state: "detached" });
    if (!usesRealServer) {
      expect(fake().requests.at(-1)).toMatchObject({
        method: "POST",
        path: "/v1/repositories/1/issues",
        body: { title: "Made in place" },
      });
    }
  });

  it.skipIf(usesRealServer)("keeps a refusal of the line in a column under it", async () => {
    await fresh();
    await openBoard();
    fake().refuseNext("POST", "/v1/repositories/1/issues", 500, "the forge refused the issue");
    const backlog = column("Backlog");
    await addFromMenu("Backlog");
    const line = backlog.getByLabel("Title of the new card in Backlog");
    await line.fill("Doomed");
    await line.press("Enter");
    await backlog.getByRole("alert").filter({ hasText: "the forge refused the issue" }).waitFor();
    expect(await line.inputValue()).toBe("Doomed");
  });

  it("opens the form of a new issue on C from another page, and keeps it for the next with Create more", async () => {
    await fresh();
    await openBoard();
    await page.goto(`${server.url}/agents`);
    await page.getByRole("heading", { level: 1 }).waitFor();
    await page.keyboard.press("c");
    const form = page.getByRole("dialog", { name: "New issue" });
    await form.waitFor();
    // A c typed into the title is typing, not a second opening.
    await form.getByLabel("Title").fill("First of two");
    await form.getByLabel("Title").press("c");
    expect(await form.getByLabel("Title").inputValue()).toBe("First of twoc");
    await form.getByLabel("Title").fill("First of two");
    await form.getByLabel("Create more").check();
    await form.getByRole("button", { name: "Create", exact: true }).click();
    await form.getByText(/^Created #\d+ First of two$/).waitFor();
    // Still open, empty and ready for the next card.
    expect(await form.getByLabel("Title").inputValue()).toBe("");
    await form.getByLabel("Title").fill("Second of two");
    await form.getByRole("button", { name: "Create", exact: true }).click();
    await form.getByText(/^Created #\d+ Second of two$/).waitFor();
    await form.getByRole("button", { name: "Cancel" }).click();
    await form.waitFor({ state: "detached" });
    await openBoard();
    await page.getByText("First of two").first().waitFor();
    await page.getByText("Second of two").first().waitFor();
  });

  it.skipIf(usesRealServer)("keeps the refusal of the form beside its title", async () => {
    await fresh();
    await openBoard();
    fake().refuseNext("POST", "/v1/repositories/1/issues", 422, "the title is already taken");
    await page.keyboard.press("c");
    const form = page.getByRole("dialog", { name: "New issue" });
    await form.getByLabel("Title").fill("Taken");
    await form.getByRole("button", { name: "Create", exact: true }).click();
    await form.getByRole("alert").filter({ hasText: "already taken" }).waitFor();
    // It stands directly under the title, before the description.
    const order = await form.locator("label, [role=alert]").allTextContents();
    const at = (text: string) => order.findIndex((t) => t.includes(text));
    expect(at("Title")).toBeLessThan(at("already taken"));
    expect(at("already taken")).toBeLessThan(at("Description"));
  });

  it.skipIf(usesRealServer)(
    "shows each card flat: status icon, id and title, and a pale line only when it runs or needs you",
    async () => {
      await fresh();
      await openBoard();
      // Columns: Backlog is always there; its name stands alone, with no count.
      await column("Backlog").waitFor();
      expect(
        await column("Not started")
          .getByTestId(/^card-\d+$/)
          .count(),
      ).toBe(2);
      expect(await column("Not started").getByRole("heading").textContent()).toBe("Not started");

      const running = card(1);
      await running.waitFor();
      await running.getByTestId("status-running").waitFor();
      expect(await running.getByTestId("card-activity").textContent()).toContain("Coder");
      // A running card has no outline of progress and casts no shadow.
      expect(await running.evaluate((el) => getComputedStyle(el).boxShadow)).toBe("none");

      // A card that needs a person says so in the orange line.
      const needs = card(2);
      await needs.getByTestId("status-needsYou").waitFor();
      expect(await needs.getByTestId("card-activity").textContent()).toBe("Plan ready");

      // An idle card is its id and its title, and nothing else.
      const idle = card(3);
      expect(await idle.getByTestId("card-activity").count()).toBe(0);
      expect(await idle.evaluate((el) => getComputedStyle(el).boxShadow)).toBe("none");
      expect(await idle.evaluate((el) => el.firstElementChild?.children.length)).toBe(2);

      await card(6).getByTestId("card-activity").getByText("Stuck").waitFor();
      await card(7).getByTestId("status-done").waitFor();
    },
  );

  it.skipIf(usesRealServer)(
    "says what an empty column holds on the column's own surface",
    async () => {
      await fresh();
      await openBoard();
      const words = column("In review").getByText(
        "A card arrives here when an agent has a change to review.",
        { exact: true },
      );
      await words.waitFor();
      // The column is the surface these words sit on. A card of its own inside a column is a second surface under the
      // same sentence, and one that reads as a card which has lost its words.
      const face = await words.evaluate((el) => {
        const box = el.closest("section");
        const column = el.closest("section[aria-label]");
        if (!box || !column) throw new Error("the empty state is not drawn in a column");
        const style = getComputedStyle(box);
        const sentence = el.getBoundingClientRect();
        const space = box.getBoundingClientRect();
        const lane = column.getBoundingClientRect();
        return {
          background: style.backgroundColor,
          shadow: style.boxShadow,
          heading: box.querySelector("h2") !== null,
          // Centred in the column, and in the room under its header rather than at the top of it. The second one only
          // reads when the state took the room: a state that only hugged its own line is the same height as its words.
          centreX: Math.abs((sentence.left + sentence.right) / 2 - (lane.left + lane.right) / 2),
          centreY: Math.abs((sentence.top + sentence.bottom) / 2 - (space.top + space.bottom) / 2),
          room: space.height - sentence.height,
        };
      });
      expect(face.background, "the empty state draws a surface of its own").toBe(
        "rgba(0, 0, 0, 0)",
      );
      expect(face.heading, "a heading repeats the name the column's header already gives").toBe(
        false,
      );
      expect(
        face.centreX,
        "the sentence sits in the middle of the column's width",
      ).toBeLessThanOrEqual(2);
      expect(
        face.room,
        "the sentence sits at the top of the column instead of in its space",
      ).toBeGreaterThan(24);
      expect(
        face.centreY,
        "the sentence is not in the middle of the room it was given",
      ).toBeLessThanOrEqual(2);
      expect(face.shadow, "the empty state lifts itself off the column").toBe("none");
      // The column is plain: no fill, and no "+" or "+ Add" in it.
      const lane = await column("In review").evaluate((el) => getComputedStyle(el).backgroundColor);
      expect(lane, "the column has a box of its own").toBe("rgba(0, 0, 0, 0)");
      expect(
        await column("In review")
          .getByRole("button", { name: /Add a card/ })
          .count(),
      ).toBe(0);
    },
  );
});

describe("column scrolling", () => {
  it.skipIf(usesRealServer)(
    "scrolls a column with many cards on its own, and nothing on the board masks",
    async () => {
      await fresh();
      await page.setViewportSize({ width: 1280, height: 720 });
      await openBoard();
      // 60 more cards, all in the Backlog.
      await page.evaluate(async () => {
        for (let at = 0; at < 60; at += 1) {
          await fetch("/v1/repositories/1/issues", {
            method: "POST",
            headers: { "Content-Type": "application/json", "X-Moebius": "1" },
            body: JSON.stringify({ title: `Filler ${at}`, body: "" }),
          });
        }
      });
      await column("Backlog").getByText("Filler 59").waitFor({ state: "attached" });
      const read = () =>
        page.evaluate(() => {
          const scrollOf = (el: Element) => el.scrollTop;
          const section = (name: string) =>
            document.querySelector(`section[aria-label="${name}"]`) as HTMLElement;
          const cards = (name: string) => section(name).querySelector("div") as HTMLElement;
          const doc = document.scrollingElement as Element;
          const main = document.querySelector("main") as HTMLElement;
          return {
            document: doc.scrollHeight - doc.clientHeight,
            main: main.scrollHeight - main.clientHeight,
            overflow: cards("Backlog").scrollHeight - cards("Backlog").clientHeight,
            backlog: scrollOf(cards("Backlog")),
            other: scrollOf(cards("In progress")),
            masked: [...main.querySelectorAll("*"), main].filter((el) => {
              const style = getComputedStyle(el);
              return (
                (style.maskImage !== "none" && style.maskImage !== "") ||
                (style.webkitMaskImage !== "none" && style.webkitMaskImage !== "")
              );
            }).length,
          };
        });
      const before = await read();
      expect(before.document, "the document scrolls").toBe(0);
      expect(before.main, "the page scrolls as one sheet").toBeLessThan(8);
      expect(before.overflow, "the column has nothing to scroll").toBeGreaterThan(200);
      expect(before.masked, "something on the board masks its scroll").toBe(0);
      const box = await column("Backlog").boundingBox();
      if (!box) throw new Error("no box");
      await page.mouse.move(box.x + box.width / 2, box.y + 200);
      await page.mouse.wheel(0, 600);
      await expect.poll(async () => (await read()).backlog).toBeGreaterThan(0);
      const after = await read();
      expect(after.other, "another column scrolled with it").toBe(0);
      expect(after.main, "the page scrolled with the column").toBeLessThan(8);
    },
  );
});

describe("live data", () => {
  it.skipIf(usesRealServer)("moves a card when the server moves it", async () => {
    await fresh();
    await openBoard();
    await column("Not started").getByTestId("card-3").waitFor();
    broadcast(fake().moveCard(3, "In progress"));
    await column("In progress").getByTestId("card-3").waitFor();
    expect(await column("Not started").getByTestId("card-3").count()).toBe(0);
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
    await (await unfolded(3)).getByText("Not started", { exact: true }).waitFor();
    broadcast(fake().moveCard(3, "In review"));
    await (await unfolded(3)).getByText("In review", { exact: true }).waitFor();
  });

  it.skipIf(!usesRealServer)("opens the card of an issue clicked on the Issues page", async () => {
    await fresh();
    await openBoard();
    const ids: number[] = [];
    for (const title of ["First issue", "Second issue"]) {
      const made = await context.request.post(`${server.url}/v1/repositories/${demo}/issues`, {
        headers: { "X-Moebius": "1" },
        data: { title },
      });
      expect(made.ok(), await made.text()).toBe(true);
      ids.push(((await made.json()) as { id: number }).id);
    }
    await page.goto(`${server.url}/issues`);
    await page.getByTestId("list-row").filter({ hasText: "First issue" }).click();
    await page.waitForURL(`**/issues?**card=${ids[0]}`);
    await dock().getByText("First issue").first().waitFor();
    await page.goBack();
    await page.getByTestId("issues-page").waitFor();
  });

  it.skipIf(usesRealServer)("links the pull request of a card from the dock", async () => {
    await fresh();
    await openBoard();
    await card(5).click();
    const pull = dock().getByRole("link", { name: "Pull request #11, open" });
    expect(await pull.getAttribute("href")).toBe("https://github.com/acme/demo/pull/11");
  });

  it.skipIf(usesRealServer)(
    "names the two waiting properties of a card by what each one is",
    async () => {
      await fresh();
      await openBoard();
      await page.goto(`${server.url}/board/1?card=4`);
      await (await unfolded(4)).getByText("Why it waits", { exact: true }).waitFor();
      await (await unfolded(4)).getByText("Waiting on", { exact: true }).waitFor();
      expect(await (await unfolded(4)).getByText("Waiting", { exact: true }).count()).toBe(0);
    },
  );

  it.skipIf(usesRealServer)(
    "edits every property of the issue with the keyboard alone",
    async () => {
      await fresh();
      await openBoard();
      await card(3).click();
      const facts = await unfolded(3);
      /** Enter opens the value's menu, the arrows move to the row and Enter saves it. */
      const choose = async (value: string, row: string) => {
        await facts.getByRole("button", { name: value }).focus();
        await page.keyboard.press("Enter");
        await page.waitForFunction(
          () => document.activeElement?.getAttribute("role") === "menuitemradio",
        );
        await arrowDownTo(page.getByRole("menuitemradio", { name: row, exact: true }));
        await page.keyboard.press("Enter");
      };
      /** Enter opens the value's field, what is typed replaces it, and Enter saves it. */
      const type = async (value: string, field: string, typed: string) => {
        await facts.getByRole("button", { name: value }).focus();
        await page.keyboard.press("Enter");
        await facts.getByRole("textbox", { name: field }).waitFor();
        await page.keyboard.press("Control+a");
        await page.keyboard.type(typed);
        await page.keyboard.press("Enter");
      };

      // A choice.
      await choose("State Open", "Closed");
      await facts.getByRole("button", { name: "State Closed" }).waitFor();
      await column("Done").getByTestId("card-3").waitFor();

      // A person.
      await choose("Claim No one", "Me");
      await facts.getByRole("button", { name: /^Claim person/ }).waitFor();

      // A number.
      await type("Parent None", "Parent", "#2");
      await facts.getByRole("button", { name: "Parent #2 Fix the login redirect" }).waitFor();

      // Text: Enter opens the field in the value's place, Enter saves, the focus comes back to the value.
      const title = facts.getByRole("button", { name: "Estimate No estimate" });
      await title.focus();
      await page.keyboard.press("Enter");
      const field = facts.getByRole("textbox", { name: "Estimate" });
      await field.waitFor();
      await page.keyboard.press("Control+a");
      await page.keyboard.type("3");
      await page.keyboard.press("Enter");
      const renamed = facts.getByRole("button", { name: "Estimate 3 points" });
      await renamed.waitFor();
      await expect.poll(() => renamed.evaluate((el) => el === document.activeElement)).toBe(true);

      // Escape leaves the field unsaved, and does not close the dock behind it.
      await page.keyboard.press("Enter");
      await field.waitFor();
      await page.keyboard.type("7");
      await page.keyboard.press("Escape");
      await renamed.waitFor();
      await dock().waitFor();
      // The focus comes back to the value a frame later; the next keys wait for it rather than race it.
      await expect.poll(() => renamed.evaluate((el) => el === document.activeElement)).toBe(true);

      // A list: Tab reaches "+", Enter opens the field, Enter adds the label.
      const add = facts.getByRole("button", { name: "Add a label" });
      await add.focus();
      await page.keyboard.press("Enter");
      // The keys go to the field once it is there, not to the button it takes the place of.
      await facts.getByRole("textbox", { name: "Add a label" }).waitFor();
      await page.keyboard.type("docs");
      await page.keyboard.press("Enter");
      const remove = facts.getByRole("button", { name: "Remove label docs" });
      await remove.waitFor();
      await remove.focus();
      await page.keyboard.press("Enter");
      await remove.waitFor({ state: "detached" });

      // And the blockers, by number.
      await facts.getByRole("button", { name: "Add a blocker by its number" }).focus();
      await page.keyboard.press("Enter");
      await facts.getByRole("textbox", { name: "Add a blocker by its number" }).waitFor();
      await page.keyboard.type("1");
      await page.keyboard.press("Enter");
      await facts.getByRole("button", { name: "Remove blocker #1" }).waitFor();
    },
  );

  it.skipIf(usesRealServer)(
    "says a refusal under the property that asked, and keeps the server's value",
    async () => {
      await fresh();
      await openBoard();
      await card(3).click();
      const facts = await unfolded(3);
      await facts.getByRole("button", { name: "Estimate No estimate" }).click();
      const field = facts.getByRole("textbox", { name: "Estimate" });
      await field.fill("0");
      await field.press("Enter");
      const refusal = facts.getByRole("alert");
      await refusal.waitFor();
      const value = facts.getByRole("button", { name: "Estimate No estimate" });
      // A person hearing the page is told it with the value it is about.
      const described = await value.getAttribute("aria-describedby");
      expect(described).toBe(await refusal.locator("..").getAttribute("id"));
    },
  );
});

describe("the keyboard", () => {
  it.skipIf(usesRealServer)(
    "makes, assigns and moves a card with the keyboard alone, and shows each key in the palette",
    async () => {
      await fresh();
      await openBoard();
      // Make: C opens the form, the title is typed and Enter creates the card.
      await page.keyboard.press("c");
      const form = page.getByRole("dialog", { name: "New issue" });
      await form.getByLabel("Title").waitFor();
      await page.keyboard.type("Typed from the keys");
      await page.keyboard.press("Enter");
      await status('Created "Typed from the keys"').waitFor();
      await page.keyboard.press("Escape");
      await form.waitFor({ state: "detached" });

      // Open the card that waits for approval, assign it and move it to Not started, all with keys.
      await card(2).focus();
      await page.keyboard.press("Enter");
      await dock().getByText("Approve: Fix the login redirect").waitFor();
      // The focus is on the card in the dock, not in a field, so its keys are the page's.
      await page.locator("body").press("a");
      await status("Assigning #2").or(status("Unassigning #2")).first().waitFor();
      await expect
        .poll(() => fake().requests.some((r) => r.path === "/v1/cards/2/claim"))
        .toBe(true);
      await page.keyboard.press("Control+k");
      const options = page.getByRole("option");
      // The palette no longer shows a key box on a row: the commands of the card are there by their words.
      await options.filter({ hasText: "Unassign" }).waitFor();
      await options.filter({ hasText: "Move to Not started" }).waitFor();
      await options.filter({ hasText: "New issue" }).waitFor();
      await page.keyboard.press("Escape");
      await options.first().waitFor({ state: "detached" });
      await page.locator("body").press("n");
      await status("Approved #2").waitFor();
      await column("Not started").getByTestId("card-2").waitFor();
    },
  );

  it.skipIf(usesRealServer)("does not act on a key typed in a field", async () => {
    await fresh();
    await openBoard();
    await page.goto(`${server.url}/board/1?card=2`);
    await dock().getByText("Approve: Fix the login redirect").waitFor();
    const before = fake().requests.length;
    await dock().getByLabel("Write a comment").fill("");
    await dock().getByLabel("Write a comment").press("a");
    await dock().getByLabel("Write a comment").press("n");
    await dock().getByLabel("Write a comment").press("?");
    expect(await dock().getByLabel("Write a comment").inputValue()).toBe("an?");
    expect(
      fake()
        .requests.slice(before)
        .filter((r) => r.method === "POST"),
    ).toEqual([]);
    expect(page.url()).not.toContain("/shortcuts");
  });

  it("lists every shortcut on a page that ? opens", async () => {
    await fresh();
    await openBoard();
    await page.locator("body").press("?");
    await page.getByRole("heading", { name: "Keyboard shortcuts", level: 1 }).waitFor();
    expect(page.url()).toContain("/shortcuts");
    for (const label of [
      "New issue",
      "Assign to me, or unassign",
      "Move to Not started: approve the issue",
      "Move to Done: merge the change",
    ]) {
      await page.getByText(label, { exact: true }).waitFor();
    }
  });
});

describe("the decisions", () => {
  it.skipIf(usesRealServer)(
    "approves an issue from the dock and moves the card to Not started",
    async () => {
      await fresh();
      await openBoard();
      await card(2).click();
      await dock().getByText("Approve: Fix the login redirect").waitFor();
      await dock().getByRole("button", { name: "Approve" }).click();
      await status("Approved #2").waitFor();
      await column("Not started").getByTestId("card-2").waitFor();
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
    // The move that carries the reason is a small change, folded with the one beside it.
    await dock()
      .getByRole("button", { name: /\d+ changes/ })
      .last()
      .click();
    // The note property under "More properties" says it too, folded away.
    await dock()
      .getByText(/The tests are missing/)
      .locator("visible=true")
      .first()
      .waitFor();
  });

  it.skipIf(usesRealServer)("retries a stuck card", async () => {
    await fresh();
    await openBoard();
    await card(6).click();
    await dock().getByRole("button", { name: "Retry" }).click();
    await status("Retrying #6.").waitFor();
    expect(fake().requests.at(-1)).toMatchObject({ method: "POST", path: "/v1/cards/6/retry" });
    await card(6).getByTestId("card-activity").waitFor();
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

describe("blocking, unblocking and deleting a card", () => {
  /** The ⋯ button of a card of the board, or of the dock (which stands beside the board). */
  const menuOf = (where: Locator, number: number) =>
    where.getByRole("button", { name: `More actions for #${number}` });
  const item = (name: string) => page.getByRole("menuitem", { name, exact: true });
  const asking = (title: string) => page.getByRole("dialog", { name: title });

  it.skipIf(usesRealServer)(
    "blocks a card from its menu with a reason, and unblocks it from the same menu",
    async () => {
      await fresh();
      await openBoard();
      // A card that waits on an issue says so in its own words, apart from the one a person blocks.
      await card(4).getByTestId("card-activity").getByText("Waiting on #3").waitFor();
      await menuOf(card(3), 3).click();
      await item("Block…").click();
      const dialog = asking("Block #3");
      await dialog.getByLabel("Reason").fill("wait for the design");
      await dialog.getByRole("button", { name: "Block", exact: true }).click();
      await status("Blocked #3. Its runs are stopped.").waitFor();
      await asking("Block #3").waitFor({ state: "detached" });
      expect(fake().requests.at(-1)).toMatchObject({
        method: "POST",
        path: "/v1/cards/3/block",
        body: { reason: "wait for the design" },
      });
      // It stays in its column, its line says Blocked, and its reason is the line's tooltip.
      const tag = column("Not started").getByTestId("card-3").getByText("Blocked", { exact: true });
      await tag.waitFor();
      expect(await tag.getAttribute("title")).toBe("wait for the design (blocked by a person)");
      // The card that waits on issues is a different state: it is not called blocked.
      expect(await card(4).getByText("Blocked", { exact: true }).count()).toBe(0);

      await menuOf(card(3), 3).click();
      expect(await item("Block…").count()).toBe(0);
      await item("Unblock").click();
      await status("Unblocked #3.").waitFor();
      await tag.waitFor({ state: "detached" });
      expect(fake().requests.at(-1)).toMatchObject({ method: "POST", path: "/v1/cards/3/unblock" });
      await menuOf(card(3), 3).click();
      await item("Block…").waitFor();
    },
  );

  it.skipIf(usesRealServer)("takes a block back with Undo in the toast", async () => {
    await fresh();
    await openBoard();
    await menuOf(card(3), 3).click();
    await item("Block…").click();
    await asking("Block #3").getByLabel("Reason").fill("by mistake");
    await asking("Block #3").getByRole("button", { name: "Block", exact: true }).click();
    const toast = status("Blocked #3. Its runs are stopped.");
    await toast.waitFor();
    const tag = column("Not started").getByTestId("card-3").getByText("Blocked", { exact: true });
    await tag.waitFor();
    await toast.getByRole("button", { name: "Undo" }).click();
    await toast.waitFor({ state: "detached" });
    await tag.waitFor({ state: "detached" });
    expect(fake().requests.at(-1)).toMatchObject({ method: "POST", path: "/v1/cards/3/unblock" });
  });

  it.skipIf(usesRealServer)("stops the run of a card that is blocked", async () => {
    await fresh();
    await openBoard();
    await card(1).getByTestId("card-activity").waitFor();
    await menuOf(card(1), 1).click();
    await item("Block…").click();
    await asking("Block #1").getByRole("button", { name: "Block", exact: true }).click();
    await status("Blocked #1").waitFor();
    await card(1).getByText("Blocked", { exact: true }).waitFor();
    // The run was stopped, so the card is no longer live.
    await card(1)
      .getByText(/^Coder · code/)
      .waitFor({ state: "detached" });
    expect(fake().requests.at(-1)).toMatchObject({ body: { reason: "" } });
  });

  it.skipIf(usesRealServer)("stops the run of a card from its menu and from the dock", async () => {
    await fresh();
    await openBoard();
    await card(1).getByTestId("card-activity").waitFor();
    await menuOf(card(1), 1).click();
    await item("Stop run").click();
    await status("Stopping #1.").waitFor();
    expect(fake().requests.at(-1)).toMatchObject({ method: "POST", path: "/v1/runs/1/stop" });
    // The card is no longer live and is not blocked, and the menu offers no second stop.
    await card(1)
      .getByText(/^Coder · code/)
      .waitFor({ state: "detached" });
    expect(await card(1).getByText("Blocked", { exact: true }).count()).toBe(0);
    await menuOf(card(1), 1).click();
    expect(await item("Stop run").count()).toBe(0);
    await page.keyboard.press("Escape");

    // A card with a live run has the button in its dock too: retrying the stuck card 6 starts one.
    await card(6).click();
    await dock().getByRole("button", { name: "Retry" }).click();
    await card(6).getByTestId("card-activity").waitFor();
    await dock().getByRole("button", { name: "Stop run" }).click();
    await status("Stopping #6.").waitFor();
    await dock().getByRole("button", { name: "Stop run" }).waitFor({ state: "detached" });
  });

  it.skipIf(usesRealServer)("blocks from the menu of the dock and shows why there", async () => {
    await fresh();
    await openBoard();
    await card(3).click();
    await (await unfolded(3)).getByText("Not started", { exact: true }).waitFor();
    await menuOf(dock(), 3).click();
    await item("Block…").click();
    await asking("Block #3").getByLabel("Reason").fill("needs a design");
    await asking("Block #3").getByRole("button", { name: "Block", exact: true }).click();
    await status("Blocked #3").waitFor();
    await (await unfolded(3)).getByText("needs a design (blocked by a person)").waitFor();
    await dock().getByText("Blocked", { exact: true }).first().waitFor();
    await menuOf(dock(), 3).click();
    await item("Unblock").click();
    await status("Unblocked #3.").waitFor();
    await (await unfolded(3))
      .getByText("needs a design (blocked by a person)")
      .waitFor({ state: "detached" });
  });

  it.skipIf(usesRealServer)("keeps the dialog open with what the server refused", async () => {
    await fresh();
    await openBoard();
    fake().refuseNext("POST", "/v1/cards/3/block", 409, "card 3 is held already");
    await menuOf(card(3), 3).click();
    await item("Block…").click();
    await asking("Block #3").getByLabel("Reason").fill("later");
    await asking("Block #3").getByRole("button", { name: "Block", exact: true }).click();
    await asking("Block #3")
      .getByRole("alert")
      .filter({ hasText: "card 3 is held already" })
      .waitFor();
    expect(await asking("Block #3").getByLabel("Reason").inputValue()).toBe("later");
    await asking("Block #3").getByRole("button", { name: "Cancel" }).click();
    await asking("Block #3").waitFor({ state: "detached" });
  });

  it.skipIf(usesRealServer)(
    "reaches the menu with the keyboard and keeps its keys from the card",
    async () => {
      await fresh();
      await openBoard();
      await menuOf(card(3), 3).focus();
      await page.keyboard.press("Enter");
      await item("Block…").waitFor();
      // Escape closes the menu and gives the focus back to its button.
      await page.keyboard.press("Escape");
      await item("Block…").waitFor({ state: "detached" });
      expect(await menuOf(card(3), 3).evaluate((el) => el === document.activeElement)).toBe(true);
      await page.keyboard.press("Enter");
      await item("Block…").focus();
      await page.keyboard.press("Enter");
      await asking("Block #3").waitFor();
      // Enter in the menu was not the card's own Enter, which opens the card in the dock.
      expect(page.url()).not.toContain("card=3");
    },
  );

  it.skipIf(usesRealServer)(
    "deletes a card from its menu after saying what goes and that its issue stays",
    async () => {
      await fresh();
      await openBoard();
      await menuOf(card(3), 3).click();
      await item("Delete…").click();
      const dialog = asking("Delete #3");
      await dialog.getByText("Document the API").waitFor();
      await dialog.getByText("worktrees and branches in the clone").waitFor();
      await dialog.getByText("The issue stays open on GitHub").waitFor();
      // Asking again is the only way to go on: nothing was sent yet.
      expect(fake().requests.some((r) => r.method === "DELETE")).toBe(false);
      await dialog.getByRole("button", { name: "Delete card" }).click();
      await status("Deleted #3.").waitFor();
      await card(3).waitFor({ state: "detached" });
      expect(fake().requests.at(-1)).toMatchObject({ method: "DELETE", path: "/v1/cards/3" });
      // The card that waited on it is left waiting, as for any issue that has no card.
      await card(4).getByTestId("card-activity").getByText("Waiting on #3").waitFor();
    },
  );

  it.skipIf(usesRealServer)("deletes the card of the dock, and the dock closes", async () => {
    await fresh();
    await openBoard();
    await card(3).click();
    await (await unfolded(3)).getByText("Not started", { exact: true }).waitFor();
    await menuOf(dock(), 3).click();
    await item("Delete…").click();
    await asking("Delete #3").getByRole("button", { name: "Delete card" }).click();
    await status("Deleted #3.").waitFor();
    await card(3).waitFor({ state: "detached" });
    // The dock does not open another card in its place.
    await dock().waitFor({ state: "detached" });
  });

  it.skipIf(usesRealServer)("does not delete when the person leaves the question", async () => {
    await fresh();
    await openBoard();
    await menuOf(card(3), 3).click();
    await item("Delete…").click();
    await asking("Delete #3").getByRole("button", { name: "Cancel" }).click();
    await asking("Delete #3").waitFor({ state: "detached" });
    await card(3).waitFor();
    expect(fake().requests.some((r) => r.method === "DELETE")).toBe(false);
  });

  it.skipIf(usesRealServer)(
    "drops a card another client deleted, and closes its dock",
    async () => {
      await fresh();
      await openBoard();
      await card(3).click();
      await (await unfolded(3)).getByText("Not started", { exact: true }).waitFor();
      const removed = await context.request.delete(`${server.url}/v1/cards/3`, {
        headers: { "X-Moebius": "1" },
      });
      expect(removed.ok()).toBe(true);
      await card(3).waitFor({ state: "detached" });
      await dock().waitFor({ state: "detached" });
    },
  );

  it.skipIf(usesRealServer)("shows a card that another client blocked", async () => {
    await fresh();
    await openBoard();
    const blocked = await context.request.post(`${server.url}/v1/cards/3/block`, {
      headers: { "X-Moebius": "1" },
      data: { reason: "paused elsewhere" },
    });
    expect(blocked.ok()).toBe(true);
    await card(3).getByText("Blocked", { exact: true }).waitFor();
    await card(3).getByText("Next").waitFor({ state: "detached" });
    // The status filter tells a blocked card from one that waits on issues.
    const filters = page.getByRole("search", { name: "Filter the cards" });
    await addFilter(page, filters, "Status", "Blocked");
    await page.waitForURL("**status=held");
    await card(3).waitFor();
    // The address changes before the board is drawn again with the filter.
    await expect.poll(() => page.getByTestId(/^card-\d+$/).count()).toBe(1);
  });
});

describe("dragging cards", () => {
  it.skipIf(usesRealServer)(
    "refuses a drop where a person may not move the card, with the reason",
    async () => {
      await fresh();
      await openBoard();
      const before = fake().requests.length;
      // Not started -> Done: four columns to the right.
      await drag(card(3), 4);
      await alert("Cannot move #3 to Done: Nobody decides this card now").waitFor();
      await column("Not started").getByTestId("card-3").waitFor();
      expect(fake().requests.length).toBe(before);
    },
  );

  it.skipIf(usesRealServer)(
    "shows a toast with the server's reason when a move fails, and the card stays",
    async () => {
      await fresh();
      await openBoard();
      fake().refuseNext("POST", "/v1/cards/2/approve", 409, "the issue changed meanwhile");
      await drag(card(2), 1);
      await alert("Could not approve #2: the issue changed meanwhile").waitFor();
      await column("Backlog").getByTestId("card-2").waitFor();
    },
  );

  it.skipIf(usesRealServer)("refuses to move a card that is working, saying so", async () => {
    await fresh();
    await openBoard();
    await drag(card(1), 3);
    await alert("Moebius is working on this card").waitFor();
    await column("In progress").getByTestId("card-1").waitFor();
  });

  it.skipIf(usesRealServer)("approves an issue dropped on Not started", async () => {
    await fresh();
    await openBoard();
    await drag(card(2), 1);
    await status("Approved #2").waitFor();
    await column("Not started").getByTestId("card-2").waitFor();
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
      // The lane scrolls smoothly, so it is in view once the scroll has run, not at the moment of the drop.
      await expect.poll(inView).toBe(true);
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
    await expect.poll(spoken).toContain("is over the column Not started");
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
    const to = (await column("Not started").boundingBox()) as {
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
    await column("Not started").getByTestId("card-2").waitFor();
  });

  it.skipIf(usesRealServer)(
    "refuses an issue that waits for approval dropped anywhere but Not started",
    async () => {
      await fresh();
      await openBoard();
      await drag(card(2), 2);
      await alert("This issue waits for your approval: drop it on Not started").waitFor();
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
    "shows the timeline, the runs and the script of the column of the card",
    async () => {
      await fresh();
      await openBoard();
      await card(1).click();
      await dock().getByRole("heading", { name: "Add a feature" }).waitFor();
      // The two moves are folded into one line, which opens to them.
      await dock()
        .getByRole("button", { name: /2 changes/ })
        .click();
      await dock().getByText("Moved from Backlog to Not started", { exact: true }).waitFor();
      await dock().getByText("Run 1 started: Coder, code").waitFor();
      const run = dock().getByRole("link", { name: "View run 1", exact: true });
      expect(await run.getAttribute("href")).toBe("/runs/1");
      await dock().getByText("projects/demo/code.luau · built in").waitFor();
      await dock().getByText('moebius.wait("nothing to do in In progress")').waitFor();
      const facts = page.getByRole("region", { name: /^Properties of #/ });
      await unfold(facts);
      await facts.getByRole("link", { name: "Coder · code", exact: true }).waitFor();
      // The state is said once, by the pill beside the title — the run line does not repeat it.
      await dock().getByText("Live", { exact: true }).waitFor();

      await dock().getByRole("link", { name: "Open the script" }).click();
      await page.waitForURL("**/board/1/scripts/In%20progress?project=1");
      await page.getByRole("heading", { name: "Script of In progress" }).waitFor();
      await page.getByText("built in").waitFor();
      await page.getByText("-- The In progress column").waitFor();
    },
  );

  it.skipIf(usesRealServer)("links a run to its page", async () => {
    await fresh();
    await openBoard();
    await card(1).click();
    await dock().getByRole("link", { name: "View run 1", exact: true }).click();
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
    const lane = (await column("Not started").boundingBox()) as { x: number; width: number };
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
    await column("Not started").waitFor();
    expect(await dock().count()).toBe(0);
  });
});

/** The tiles on the board, counted once the page has redrawn. The filters live in the address, so a choice is taken
 * by a route before the columns follow it; counting at once can read the board as it was. */
const shownIs = (count: number) =>
  expect
    .poll(() => page.getByTestId(/^(card-\d+|list-row)$/).count(), {
      timeout: 10_000,
      message: `${count} cards`,
    })
    .toBe(count);

const bar = () => page.getByRole("search", { name: "Filter the cards" });
/** The chip of a filter that is set, as its words say it. */
const chip = (words: string) => bar().getByRole("button", { name: words, exact: true });
const display = (choice: string, kind: "menuitemradio" | "menuitemcheckbox" = "menuitemradio") =>
  page.getByRole(kind, { name: choice, exact: true });

describe("the filters", () => {
  it.skipIf(usesRealServer)(
    "filters by column, agent and status with chips, and clears",
    async () => {
      await fresh();
      await openBoard();
      expect(await page.getByTestId(/^card-\d+$/).count()).toBe(7);

      await addFilter(page, bar(), "Column", "Done");
      await shownIs(1);
      await card(7).waitFor();
      expect(await column("Not started").count()).toBe(0);
      await page.waitForURL("**column=Done");
      await chip("Column is Done").waitFor();

      await bar().getByRole("button", { name: "Clear filters" }).click();
      await shownIs(7);
      await card(3).waitFor();
      expect(await chip("Column is Done").count()).toBe(0);

      await addFilter(page, bar(), "Agent", "Reviewer");
      await shownIs(1);
      await card(6).waitFor();
      // A chip is removed by its own button.
      await bar().getByRole("button", { name: "Remove filter Agent is Reviewer" }).click();
      await shownIs(7);

      await addFilter(page, bar(), "Status", "Your turn");
      await shownIs(2);
      await card(2).waitFor();
      await card(5).waitFor();

      // A chip opens its values: the filter changes without being removed first.
      await chip("Status is Your turn").click();
      await display("Waiting on issues").click();
      await shownIs(1);
      await card(4).waitFor();

      // Two filters together, and a reload keeps them.
      await chip("Status is Waiting on issues").click();
      await display("Stuck").click();
      await addFilter(page, bar(), "Agent", "Coder");
      await shownIs(0);
      await page.reload();
      await shownIs(0);
      await chip("Agent is Coder").waitFor();
      await chip("Status is Stuck").waitFor();
    },
  );

  it.skipIf(usesRealServer)("sets and removes a filter with the keyboard alone", async () => {
    await fresh();
    await openBoard();
    await bar().getByRole("button", { name: "Filter", exact: true }).focus();
    // Enter opens the fields, the arrow down moves to Agent, the arrow right opens its values and Enter chooses one.
    await page.keyboard.press("Enter");
    await page.getByRole("menuitem", { name: "Column", exact: true }).waitFor();
    const agent = page.getByRole("menuitem", { name: "Agent", exact: true });
    await arrowDownTo(agent);
    await page.keyboard.press("ArrowRight");
    await display("Coder").waitFor();
    // The sub-menu takes focus a moment after it shows; arrow keys before that go to the wrong menu.
    await page.waitForFunction(
      () => document.activeElement?.getAttribute("role") === "menuitemradio",
    );
    const reviewer = display("Reviewer");
    await arrowDownTo(reviewer);
    await page.keyboard.press("Enter");
    await chip("Agent is Reviewer").waitFor();
    await shownIs(1);
    await card(6).waitFor();
    // The chip's own button is reached with the keyboard too.
    await bar().getByRole("button", { name: "Remove filter Agent is Reviewer" }).focus();
    await page.keyboard.press("Enter");
    await shownIs(7);
    expect(await chip("Agent is Reviewer").count()).toBe(0);
  });

  it.skipIf(usesRealServer)(
    "offers Save view only once something is set, and saves it",
    async () => {
      await fresh();
      await openBoard();
      const save = bar().getByRole("button", { name: "Save view" });
      expect(await save.count()).toBe(0);
      await addFilter(page, bar(), "Column", "Backlog");
      await save.click();
      await status(/Saved this view as "View 3"/).waitFor();
      if (!("views" in server)) throw new Error("this test needs the fake server");
      const fake = server.views;
      expect(fake.requests.at(-1)).toMatchObject({
        method: "POST",
        path: "/v1/views",
        body: { shared: false, layout: "board", filter: { columns: ["Backlog"] } },
      });
    },
  );

  it.skipIf(usesRealServer)(
    "keeps the agent, status and due filters in a saved view, and opens it on the board with them set",
    async () => {
      await fresh();
      await openBoard();
      await addFilter(page, bar(), "Agent", "Reviewer");
      await chip("Agent is Reviewer").waitFor();
      await shownIs(1);
      await bar().getByRole("button", { name: "Save view" }).click();
      await status(/Saved this view as "View 3"/).waitFor();
      // The notice no longer says a filter was left out, and the view carries the agent.
      expect(await status(/without/).count()).toBe(0);
      if (!("views" in server)) throw new Error("this test needs the fake server");
      expect(server.views.requests.at(-1)).toMatchObject({
        method: "POST",
        path: "/v1/views",
        body: { layout: "board", filter: { repositories: [1], agents: ["Reviewer"], due: "" } },
      });
      // Opened from the Views page, the view is the board it was saved from: its filter is set and its name is in the bar.
      await page.goto(`${server.url}/views`);
      await page.getByRole("link", { name: /View 3/ }).click();
      await page.getByTestId("board-view").waitFor();
      await page.waitForURL(/\/board\/1\?.*view=3/);
      await chip("Agent is Reviewer").waitFor();
      await shownIs(1);
      expect(await page.getByTestId("board-view").textContent()).toContain("View 3");
      // Changing the filter says the view is no longer what is shown, and closing it leaves the filter alone.
      await bar().getByRole("button", { name: "Remove filter Agent is Reviewer" }).click();
      await shownIs(7);
      await page.getByTestId("board-view").getByText("(filters changed)").waitFor();
      await page.getByRole("button", { name: "Close view" }).click();
      await page.getByTestId("board-view").waitFor({ state: "detached" });
      // Its page is still there to change or delete it.
      await page.goto(`${server.url}/views/3?manage`);
      await page.getByTestId("view").waitFor();
      await page.getByRole("link", { name: "Open on the board" }).waitFor();
    },
  );
});

describe("Display", () => {
  it.skipIf(usesRealServer)(
    "switches between the board and the list, and a reload keeps it",
    async () => {
      await fresh();
      await openBoard();
      await column("Not started").waitFor();
      await bar().getByRole("button", { name: "Display" }).click();
      await display("List").click();
      await page.waitForURL("**layout=list");
      // The lanes are gone and every card is a row under the heading of its column.
      await page.getByTestId("group-row").filter({ hasText: "Done" }).waitFor();
      expect(await page.getByTestId("list-row").count()).toBe(7);
      await shownIs(7);
      await page.reload();
      await shownIs(7);
      expect(await page.getByTestId("list-row").count()).toBe(7);

      // A row opens the card, and a filter still applies.
      await addFilter(page, bar(), "Agent", "Reviewer");
      await shownIs(1);
      await page.getByTestId("list-row").click();
      await dock().waitFor();
      await page.keyboard.press("Escape");
      await dock().waitFor({ state: "detached" });

      await bar().getByRole("button", { name: "Display" }).click();
      await display("Board").click();
      await page.waitForURL((url) => !url.search.includes("layout="));
      expect(await page.getByTestId("list-row").count()).toBe(0);
      await card(6).waitFor();
    },
  );

  it.skipIf(usesRealServer)("groups the list, and sorts it", async () => {
    await fresh();
    await openBoard();
    await page.goto(`${server.url}${new URL(page.url()).pathname}?layout=list`);
    await shownIs(7);
    await bar().getByRole("button", { name: "Display" }).click();
    await display("Status").click();
    await page.waitForURL("**group=status");
    await page.getByTestId("group-row").filter({ hasText: "Your turn" }).waitFor();
    expect(await page.getByTestId("group-row").filter({ hasText: "Not started" }).count()).toBe(0);

    await bar().getByRole("button", { name: "Display" }).click();
    await display("None").click();
    await page.waitForURL("**group=none");
    await bar().getByRole("button", { name: "Display" }).click();
    await display("Title").click();
    await page.waitForURL("**sort=title");
    await page.reload();
    await shownIs(7);
    const titles = await page
      .getByTestId("list-row")
      .evaluateAll((rows) =>
        rows.map((row) => row.querySelector("span.font-medium")?.textContent ?? ""),
      );
    expect(titles).toHaveLength(7);
    expect(titles).toEqual([...titles].sort((x, y) => x.localeCompare(y)));
  });

  it.skipIf(usesRealServer)("switches off the fields a card shows", async () => {
    await fresh();
    await openBoard();
    await bar().getByRole("button", { name: "Display" }).click();
    await display("List").click();
    await page.waitForURL("**layout=list");
    const row = page.getByTestId("list-row").filter({ hasText: "Add a feature" });
    const agent = row.getByText("Coder", { exact: true });
    await agent.waitFor();
    await bar().getByRole("button", { name: "Display" }).click();
    await display("Agent and status", "menuitemcheckbox").click();
    await page.keyboard.press("Escape");
    await page.waitForURL("**hide=detail");
    await expect.poll(() => agent.count()).toBe(0);
    await page.reload();
    await bar().getByRole("button", { name: "Display" }).click();
    expect(await display("Agent and status", "menuitemcheckbox").getAttribute("aria-checked")).toBe(
      "false",
    );
    // The choices only the card of the board used, and the ones the list no longer shows, are gone.
    for (const gone of ["Description", "Labels", "Claimed by", "Round limit"]) {
      expect(await display(gone, "menuitemcheckbox").count()).toBe(0);
    }
  });
});

describe("the card open in the dock and its address", () => {
  it.skipIf(usesRealServer)(
    "closes only the menu on the first Escape and the card on the second",
    async () => {
      await fresh();
      await page.goto(`${server.url}/board/1?card=3`);
      await unfolded(3);
      const openers: [string, () => Locator][] = [
        ["Priority", () => properties(3).getByRole("button", { name: /^Priority/ })],
        ["Cycle", () => properties(3).getByRole("button", { name: /^Cycle/ })],
        ["Assignee", () => properties(3).getByRole("button", { name: /^Assignee/ })],
        ["Milestone", () => properties(3).getByRole("button", { name: /^Milestone/ })],
        ["Labels", () => properties(3).getByRole("button", { name: "Add a label" })],
        ["Due date", () => properties(3).getByRole("button", { name: /^Due date/ })],
      ];
      for (const [name, opener] of openers) {
        await opener().click();
        // Whatever opened is the only thing the key may close.
        await page.keyboard.press("Escape");
        await page.getByRole("menu").waitFor({ state: "detached" });
        expect(page.url(), name).toContain("card=3");
        await dock().waitFor();
      }
      await dock().focus();
      await page.keyboard.press("Escape");
      await dock().waitFor({ state: "detached" });
    },
  );

  it.skipIf(usesRealServer)("takes the card out of the address when it is closed", async () => {
    await fresh();
    await page.goto(`${server.url}/board/1?card=3`);
    await dock().waitFor();
    await page.getByRole("button", { name: "Close the inspector" }).click();
    await dock().waitFor({ state: "detached" });
    expect(page.url()).not.toContain("card=");
    // A reload of the board with no card open shows no card.
    await page.reload();
    await page.getByRole("heading", { name: "demo", level: 1 }).waitFor();
    await card(3).waitFor();
    expect(await dock().count()).toBe(0);
  });
});

describe("the repositories", () => {
  it.skipIf(usesRealServer)(
    "lists the repositories and opens the board of the one chosen",
    async () => {
      await fresh();
      await openBoard();
      const filters = page.getByRole("search", { name: "Filter the cards" });
      // The trigger shows the board's name, and its list holds the one board there is.
      expect(await showing(filters.getByLabel("Repository"))).toBe("demo");
      expect(await offers(page, filters.getByLabel("Repository"))).toEqual(["demo"]);
    },
  );

  it("says there is no such board for a repository that does not exist", async () => {
    await fresh();
    await page.goto(`${server.url}/board/99999`);
    await page.getByText("There is no such board").waitFor();
  });
});
