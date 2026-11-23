/**
 * Working with people on other servers, in a real browser (ADR 0027), against the fake server: an issue someone took on
 * another server is greyed with "Taken by @alice"; a pull request someone else opened is followed by a card in
 * review, which has "Review with agent" and "Merge" where the card window asks for a person's turn; the repository's
 * "Working with others" box saves, commands in issue comments included (ADR 0028): an issue taken with /take is greyed
 * with "Taken by @alice with /take" and the owner may release it from the card, and an assignee's /done waits in the card
 * window as "Moebius needs you" with the three answers; the Profile keeps the person's logins. Set MOEBIUS_SHOT_DIR to also write the pictures
 * of the board, the followed card and its window (light and dark, 1280 px wide, twice the pixels).
 */
import { mkdirSync } from "node:fs";
import { join } from "node:path";
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

const SHOTS = process.env.MOEBIUS_SHOT_DIR;

async function open(scheme: "light" | "dark" = "light") {
  server = await startServer();
  if (!server.board) throw new Error("this test needs the fake server");
  const made = server.board.seedElsewhere();
  const commands = server.board.seedCommands();
  context = await browser.newContext({
    viewport: { width: 1280, height: 900 },
    colorScheme: scheme,
    deviceScaleFactor: 2,
  });
  page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  await page.goto(`${server.url}/projects/1`);
  await page.getByRole("heading", { name: "demo", level: 1 }).waitFor();
  return { ...made, ...commands };
}

describe.skipIf(usesRealServer)("working with others", () => {
  it("greys an issue someone else took, and shows a followed pull request as an own change in review", async () => {
    const { taken, followed } = await open();
    const takenCard = page.getByTestId(`card-${taken}`);
    await takenCard.waitFor();
    expect(await takenCard.getByTestId("card-activity").textContent()).toBe("Taken by @alice");
    // The whole card is dimmed and flat, and the followed one, which is a card to act on, is not.
    expect(await takenCard.evaluate((n) => getComputedStyle(n).opacity)).toBe("0.6");
    const followedCard = page.getByTestId(`card-${followed}`);
    expect(await followedCard.evaluate((n) => getComputedStyle(n).opacity)).toBe("1");
    // Its person's avatar sits where an assignee's does on any card: the same distance from the card's right edge.
    const gap = (card: typeof takenCard) =>
      card.evaluate((n) => {
        const mark = n.querySelector('[role="img"]')?.getBoundingClientRect();
        return Math.round(n.getBoundingClientRect().right - (mark?.right ?? 0));
      });
    const assigned = page.getByTestId(`card-${taken + 2}`);
    expect(await gap(takenCard)).toBe(await gap(assigned));
    expect(await gap(followedCard)).toBe(await gap(assigned));
    expect(await followedCard.getByTestId("card-activity").textContent()).toBe(
      "Review @alice's pull request",
    );
    // The same column as the cards of this server's own that are in review.
    await page
      .getByRole("region", { name: "In review", exact: true })
      .getByTestId(`card-${followed}`)
      .waitFor();
    if (SHOTS) await shoot("board");

    // The card window has the pull request as every change has it, and the actions where a change's are.
    await followedCard.click();
    const window = page.locator("[data-card-window]");
    await window.getByTestId("elsewhere").waitFor();
    await window.getByLabel("Pull request #7, open").waitFor();
    await window.getByRole("button", { name: "Review with agent" }).waitFor();
    await window.getByRole("button", { name: "Merge" }).waitFor();
    if (SHOTS) await shoot("card-window");

    // Review with the agent asks the server and says so.
    await window.getByRole("button", { name: "Review with agent" }).click();
    await page.getByText(/The Reviewer is reading the pull request of #/).waitFor();
    expect(server.board?.requests.some((r) => r.path === "/v1/changes/30/review")).toBe(true);

    // Merging moves the card to Done.
    await window.getByRole("button", { name: "Merge" }).click();
    await page.getByText(/^Merged #/).waitFor();
    expect(server.board?.requests.some((r) => r.path === "/v1/changes/30/merge")).toBe(true);
  });

  it("shows the review of a followed pull request: verdict, summary, comments on lines and the way to the forge", async () => {
    const { followed } = await open();
    await page.getByTestId(`card-${followed}`).click();
    const review = page.locator("[data-card-window]").getByTestId("external-review");
    await review.waitFor();
    expect(await review.textContent()).toContain("Changes requested");
    expect(await review.textContent()).toContain("Round 1 of 3");
    expect(await review.textContent()).toContain("The cache is never emptied");
    const comments = review.getByTestId("review-comment");
    expect(await comments.count()).toBe(2);
    expect(await comments.first().textContent()).toContain("src/cache.rs line 42");
    const link = review.getByRole("link", { name: "Open on GitHub" });
    expect(await link.getAttribute("href")).toBe(
      "https://github.test/acme/demo/pull/7#pullrequestreview-91",
    );
    if (SHOTS) await shoot("external-review-window");
  });

  it("says why an issue is taken in its window", async () => {
    const { taken } = await open();
    await page.getByTestId(`card-${taken}`).click();
    const note = page.locator("[data-card-window]").getByTestId("taken-note");
    expect(await note.textContent()).toContain("Taken by @alice on GitHub");
  });

  it("saves how the repository works with others", async () => {
    await open();
    await page.goto(`${server.url}/projects/1/repositories/1`);
    const box = page.getByTestId("working-with-others");
    await box.waitFor();
    await box.getByLabel("Mark ready issues on GitHub").click();
    await box.getByLabel("Label").fill("Go");
    await box.getByRole("button", { name: "Save settings" }).click();
    // Saved: the box has what was sent, so there is nothing left to save.
    await page.waitForFunction(() =>
      document
        .querySelector('[data-testid="working-with-others"] button[type="submit"]')
        ?.hasAttribute("disabled"),
    );
    expect(await box.getByLabel("Label").inputValue()).toBe("Go");
  });

  it("saves the commands in issue comments with the limit and the answers", async () => {
    await open();
    await page.goto(`${server.url}/projects/1/repositories/1`);
    const part = page.getByTestId("issue-commands");
    await part.waitFor();
    await part.getByLabel("Take issues with /take, /release and /done").click();
    await part.getByLabel("Issues one login may hold").fill("2");
    await part.getByLabel("Answer on GitHub").click();
    await page
      .getByTestId("working-with-others")
      .getByRole("button", { name: "Save settings" })
      .click();
    // Saved: the part has what was sent, so there is nothing left to save.
    await page.waitForFunction(() =>
      document
        .querySelector('[data-testid="working-with-others"] button[type="submit"]')
        ?.hasAttribute("disabled"),
    );
    expect(await part.getByLabel("Issues one login may hold").inputValue()).toBe("2");
    expect(
      await part
        .getByLabel("Take issues with /take, /release and /done")
        .getAttribute("aria-checked"),
    ).toBe("true");
    expect(await part.getByLabel("Answer on GitHub").getAttribute("aria-checked")).toBe("false");
  });

  it("saves whether the server merges the pull requests the reviewer approves", async () => {
    await open();
    await page.goto(`${server.url}/projects/1/repositories/1`);
    const part = page.getByTestId("external-pull-requests");
    await part.waitFor();
    const merge = part.getByLabel("Merge external pull requests the reviewer approves");
    // On until the owner turns it off.
    expect(await merge.getAttribute("aria-checked")).toBe("true");
    expect(await part.textContent()).toContain("Foreign code then lands without a person looking");
    await merge.click();
    await page
      .getByTestId("working-with-others")
      .getByRole("button", { name: "Save settings" })
      .click();
    await page.waitForFunction(() =>
      document
        .querySelector('[data-testid="working-with-others"] button[type="submit"]')
        ?.hasAttribute("disabled"),
    );
    expect(await merge.getAttribute("aria-checked")).toBe("false");
    // It was kept: the page shows it again after a reload.
    await page.reload();
    await part.waitFor();
    expect(
      await part
        .getByLabel("Merge external pull requests the reviewer approves")
        .getAttribute("aria-checked"),
    ).toBe("false");
  });

  it("greys an issue taken with /take, and the owner may release it from the card", async () => {
    const { takenWith } = await open();
    const card = page.getByTestId(`card-${takenWith}`);
    await card.waitFor();
    expect(await card.getByTestId("card-activity").textContent()).toBe(
      "Taken by @alice with /take",
    );
    expect(await card.evaluate((n) => getComputedStyle(n).opacity)).toBe("0.6");
    if (SHOTS) await shoot("taken-with-take-board");
    await card.click();
    const window = page.locator("[data-card-window]");
    expect(await window.getByTestId("taken-note").textContent()).toContain(
      "Taken by @alice with /take on GitHub",
    );
    if (SHOTS) await shoot("taken-with-take-window");
    await window.getByRole("button", { name: "Release from @alice" }).click();
    await page.getByText(/^Released #/).waitFor();
    expect(
      server.board?.requests.some((r) => r.path === `/v1/cards/${takenWith}/release-taken`),
    ).toBe(true);
    await window
      .getByRole("button", { name: "Release from @alice" })
      .waitFor({ state: "detached" });
  });

  it("asks the owner what to do with an issue its assignee says is done, and answers on the card", async () => {
    const { asked } = await open();
    await page.getByTestId(`card-${asked}`).click();
    const window = page.locator("[data-card-window]");
    const block = window.getByTestId("needs-you-block");
    await block.waitFor();
    expect(await block.textContent()).toContain("@alice says");
    expect(await block.getByTestId("done-reason").textContent()).toContain(
      "The assets are cached now.",
    );
    await block.getByRole("button", { name: "Give it back to @alice" }).waitFor();
    await block.getByRole("button", { name: "Release it" }).waitFor();
    // The same action is not offered twice: the top of the window has no release while the question is open.
    expect(await window.getByRole("button", { name: "Release from @alice" }).count()).toBe(0);
    if (SHOTS) await shoot("done-question-window");
    await block.getByRole("button", { name: "Close the issue" }).click();
    await page.getByText(/^Closed #/).waitFor();
    expect(server.board?.requests.find((r) => r.path === `/v1/cards/${asked}/done`)?.body).toEqual({
      answer: "close",
    });
  });

  it("keeps the person's login on GitHub in the Profile", async () => {
    await open();
    await page.goto(`${server.url}/profile`);
    const field = page.getByLabel("Your login on GitHub");
    await field.fill("lea-gh");
    await page.getByRole("button", { name: "Save logins" }).click();
    await expect
      .poll(() => server.board?.requests.find((r) => r.path === "/v1/logins")?.body)
      .toEqual({ github: "lea-gh", gitlab: "", forgejo: "" });
  });
});

describe.skipIf(usesRealServer || !SHOTS)("the pictures", () => {
  for (const scheme of ["light", "dark"] as const) {
    it(`takes them in the ${scheme} theme`, async () => {
      const { followed, takenWith, asked } = await open(scheme);
      await page.getByTestId(`card-${followed}`).waitFor();
      await shoot(`board-${scheme}`);
      await page.getByTestId(`card-${followed}`).click();
      await page.locator("[data-card-window]").getByTestId("elsewhere").waitFor();
      await shoot(`card-window-${scheme}`);
      await page.goto(`${server.url}/profile`);
      await page.getByLabel("Your login on GitHub").waitFor();
      await shoot(`profile-${scheme}`);
      await page.goto(`${server.url}/projects/1/repositories/1`);
      await page.getByTestId("working-with-others").waitFor();
      await shoot(`working-with-others-${scheme}`);
      await page
        .getByTestId("working-with-others")
        .getByRole("button", { name: "Save settings" })
        .scrollIntoViewIfNeeded();
      await shoot(`settings-commands-${scheme}`);
      await page.goto(`${server.url}/projects/1`);
      await page.getByTestId(`card-${takenWith}`).waitFor();
      await shoot(`taken-with-take-board-${scheme}`);
      await page.getByTestId(`card-${takenWith}`).click();
      await page.locator("[data-card-window]").getByTestId("taken-note").waitFor();
      await shoot(`taken-with-take-window-${scheme}`);
      await page.goto(`${server.url}/projects/1`);
      await page.getByTestId(`card-${asked}`).click();
      await page.locator("[data-card-window]").getByTestId("done-reason").waitFor();
      await shoot(`done-question-window-${scheme}`);
    });
  }
});

async function shoot(name: string) {
  if (!SHOTS) return;
  mkdirSync(SHOTS, { recursive: true });
  await page.screenshot({ path: join(SHOTS, `${name}.png`) });
}
