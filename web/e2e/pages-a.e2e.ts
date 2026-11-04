/**
 * The Inbox, Projects, Fleet, Usage, Get started and Settings pages in a real browser: what each shows, and that each
 * action sends its request and shows the outcome, or the server's own sentence when it refuses. The tests follow the
 * tests of the desktop app's pages (`crates/moebius-desktop/tests/`). The fake server (`fake/pages-a.ts` next to the
 * board's) has the data; every page also loads against a real server, and the tests that need the fake's data are
 * skipped there. Every wait is on something the page shows, never on time.
 */
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type Browser, type BrowserContext, chromium, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { pickOption, showing } from "./controls";
import type { PagesAFake } from "./fake/pages-a";
import { startServer, type TestServer, usesRealServer } from "./server";

let browser: Browser;
let server: TestServer;
let context: BrowserContext;
let page: Page;
const directories: string[] = [];

/**
 * The chips that pick a repository. Both filter rows have a chip called All, so the test asks the
 * fieldset the legend names, and wants the chip whose name is nothing but the word.
 */
const repositories = (page: Page) =>
  page.locator("fieldset").filter({ has: page.locator("legend", { hasText: "Repository" }) });

beforeAll(async () => {
  browser = await chromium.launch({
    executablePath: process.env.MOEBIUS_E2E_CHROMIUM || undefined,
  });
});

afterAll(async () => {
  await browser.close();
});

/** A new server and a new browser context that may use the clipboard, signed in and on `path`. */
async function open(path: string) {
  server = await startServer();
  context = await browser.newContext({ viewport: { width: 1400, height: 1000 } });
  await context.grantPermissions(["clipboard-read", "clipboard-write"], { origin: server.url });
  page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  if (path !== "/inbox") await page.goto(`${server.url}${path}`);
}

afterEach(async () => {
  await context.close();
  await server.stop().catch(() => {});
  for (const directory of directories.splice(0))
    rmSync(directory, { recursive: true, force: true });
});

/** The fake's routes of these pages; only the tests that skip a real server ask for it. */
function fake(): PagesAFake {
  if (!server.pagesA) throw new Error("this test needs the fake server");
  return server.pagesA;
}
const board = () => {
  if (!server.board) throw new Error("this test needs the fake server");
  return server.board;
};

/** The body of the last request the fake saw with `method` to `path`. */
function lastBody(
  requests: Array<{ method: string; path: string; body: unknown }>,
  method: string,
  path: string,
) {
  return requests.findLast((request) => request.method === method && request.path === path)?.body;
}

const alert = (text: string | RegExp) => page.getByRole("alert").filter({ hasText: text });
const items = () => page.getByTestId("inbox-item");
const item = (title: string | RegExp) => items().filter({ hasText: title });
const path = () => new URL(page.url()).pathname + new URL(page.url()).search;

/** A new folder, which the server makes a repository of. */
function folder(): string {
  const directory = mkdtempSync(join(tmpdir(), "moebius-e2e-repo-"));
  directories.push(directory);
  return directory;
}

describe("the inbox", () => {
  it("loads for a real server and for the fake", async () => {
    await open("/inbox");
    await page.getByRole("heading", { name: "Inbox", level: 1 }).waitFor();
    if (usesRealServer) {
      await page.getByRole("heading", { name: "All caught up" }).waitFor();
      await page.getByText(/Nothing is waiting on you\./).waitFor();
    } else {
      await page.getByText(/things need you/).waitFor();
    }
  });

  it.skipIf(usesRealServer)(
    "shows every kind of item with its tag, source and sentence",
    async () => {
      await open("/inbox");
      await items().first().waitFor();
      // Items about cards that the board has too: a pull request closed without a merge, and a kind of a column script.
      fake().addInboxItem({
        kind: "change_closed",
        card_id: 7,
        title: "Ship the first release",
        data: { change: 2, pull_request: 18 },
      });
      fake().addInboxItem({
        kind: "needs_design",
        card_id: 3,
        title: "Needs a mockup",
        data: { reason: "no mockup yet" },
      });
      server.broadcast?.({ kind: "card_updated", data: {} });
      await page.getByText("11 things need you.").waitFor();
      expect(await items().count()).toBe(11);
      const kinds = [
        [
          "Outside issue",
          "Fix the login redirect",
          "A person has to approve this issue: An outside author opened it.",
        ],
        ["Ready to merge", "Speed up the build is ready to merge", "CI green"],
        ["Stuck", "Stop the flaky test is stuck", "Stopped: reached the round limit."],
        [
          "Retro proposal",
          "Let the Coder read the tests first",
          "Regression set: past merged cards it must not break",
        ],
        [
          "Retro proposal",
          "Raise the round limit to 7",
          "The Retro proposes this change to moebius.toml.",
        ],
        ["Decision model", "The decision model can take over tool_call", "95% of 120 judgements"],
        [
          "Issues from a grilling",
          "3 issues from a grilling",
          "The Grilling partner ended in these issues.",
        ],
        [
          "Report from a grilling",
          "A report from a grilling",
          "The export is slow because of the join.",
        ],
        ["Change from a chat", "Add a retry helper", "Harness configuration · chat"],
        [
          "Closed without a merge",
          "Ship the first release",
          "Pull request #18 was closed without a merge.",
        ],
        ["Needs design", "Needs a mockup", "no mockup yet"],
      ] as const;
      for (const [tag, title, words] of kinds) {
        const one = item(title);
        await one.getByText(tag, { exact: true }).waitFor();
        await one.getByText(words).first().waitFor();
      }
      // The facts of a change that waits, the diff of a proposal for the configuration and the trials of one for the harness.
      const merge = item("Speed up the build");
      await merge.getByText("2", { exact: true }).waitFor();
      await merge.getByText("96k tok").waitFor();
      await merge.getByText("moebius/card-5").waitFor();
      await item("Raise the round limit").getByText("+round_limit = 7").waitFor();
      await item("Let the Coder read").getByText("It passed in one round.").waitFor();
      // Newest first.
      expect(await items().first().getAttribute("data-kind")).toBe("needs_design");
    },
  );

  it.skipIf(usesRealServer)(
    "shows what a proposal offers and opens the Retro it came from",
    async () => {
      await open("/inbox");
      await items().first().waitFor();
      // The item a trial ends with names its proposal, so the card reads the proposal for what it offers and stops
      // at three lines of it: the whole of the argument is on the Retro page, which the preview itself opens.
      const harness = item("Let the Coder read");
      const reason = harness.getByRole("button", {
        name: /its instructions now say to read the tests/,
      });
      await reason.waitFor();
      expect(await reason.locator(".clamp-3").count()).toBe(1);
      // Where the change lives is the Retro page's business, not the card's.
      expect(await harness.getByText("proposal/2026-10-25-read-tests").count()).toBe(0);
      // A trial that names no card only says there was nothing to try it on, so it is not on the card. The trials
      // that ran are words beside the card they ran on, and the kind's word is the card's only pill.
      expect(await harness.getByText("No card", { exact: true }).count()).toBe(0);
      await harness.getByText("Card 3").waitFor();
      await harness.getByText("It passed in one round.").waitFor();
      expect(await harness.getByText("Positive", { exact: true }).count()).toBe(1);
      expect(await harness.getByText("Retro", { exact: true }).count()).toBe(0);
      // The preview lands on the proposal itself, not at the top of the day.
      await reason.click();
      expect(path()).toBe("/retro?day=2026-10-25&proposal=4");
      await page.getByText("proposal/2026-10-25-read-tests").waitFor();
    },
  );

  it.skipIf(usesRealServer)(
    "filters by kind and by repository, with the counts of the other filter",
    async () => {
      await open("/inbox");
      await items().first().waitFor();
      const made = await context.request.post(`${server.url}/v1/repositories`, {
        headers: { "X-Moebius": "1" },
        data: { name: "second", url: "/srv/git/second" },
      });
      expect(made.ok()).toBe(true);
      await repositories(page).getByRole("button", { name: "All", exact: true }).waitFor();
      await page.getByRole("button", { name: "All 9" }).waitFor();

      await page.getByRole("button", { name: "Stuck 1" }).click();
      // Counted only once the page says it filtered, so the count is not of the list before the click.
      await page.getByText("9 things need you; 1 shown.").waitFor();
      expect(await items().count()).toBe(1);
      expect(await page.getByRole("button", { name: "Stuck 1" }).getAttribute("aria-pressed")).toBe(
        "true",
      );
      expect(path()).toBe("/inbox?kind=stuck");

      // A repository's count is of the kind chosen.
      await page.getByRole("button", { name: "second 0" }).click();
      await page.getByText("No item matches the filter.").waitFor();
      await page.getByRole("button", { name: "demo 1" }).click();
      await page.getByText("No item matches the filter.").waitFor({ state: "detached" });
      await page.getByText("9 things need you; 1 shown.").waitFor();
      expect(await items().count()).toBe(1);
      await repositories(page).getByRole("button", { name: "All", exact: true }).click();
      await page.getByRole("button", { name: /^All \d+$/ }).click();
      await page.getByText("9 things need you.").waitFor();
      expect(await items().count()).toBe(9);
    },
  );

  it.skipIf(usesRealServer)("approves an outside issue and the item leaves", async () => {
    await open("/inbox");
    await item("Fix the login redirect").getByRole("button", { name: "Approve" }).click();
    await items().filter({ hasText: "Fix the login redirect" }).waitFor({ state: "detached" });
    expect(board().requests.some((r) => r.path === "/v1/cards/2/approve")).toBe(true);
  });

  it.skipIf(usesRealServer)(
    "merges a change, and shows the sentence of the server when it refuses",
    async () => {
      await open("/inbox");
      board().refuseNext("POST", "/v1/changes/1/merge", 422, "the branch does not merge cleanly");
      const merge = item("Speed up the build");
      await merge.getByRole("button", { name: "Merge" }).click();
      await merge
        .getByRole("alert")
        .filter({ hasText: "the branch does not merge cleanly" })
        .waitFor();
      // The buttons are back and the item stays; the second try goes through.
      await merge.getByRole("button", { name: "Merge" }).click();
      await merge.waitFor({ state: "detached" });
      expect(board().requests.filter((r) => r.path === "/v1/changes/1/merge")).toHaveLength(2);
    },
  );

  it.skipIf(usesRealServer)("asks for a reason before it sends a change back", async () => {
    await open("/inbox");
    const merge = item("Speed up the build");
    await merge.getByRole("button", { name: "Reject with a reason" }).click();
    expect(await merge.getByRole("button", { name: "Send back" }).isDisabled()).toBe(true);
    await merge.getByRole("button", { name: "Cancel" }).click();
    await merge.getByRole("button", { name: "Reject with a reason" }).waitFor();
    await merge.getByRole("button", { name: "Reject with a reason" }).click();
    await merge.getByLabel("Why? The next round is told").fill("Add a test for the cache.");
    await merge.getByRole("button", { name: "Send back" }).click();
    await merge.waitFor({ state: "detached" });
    expect(lastBody(board().requests, "POST", "/v1/changes/1/reject")).toEqual({
      reason: "Add a test for the cache.",
    });
  });

  it.skipIf(usesRealServer)("retries a stuck card", async () => {
    await open("/inbox");
    await item("Stop the flaky test").getByRole("button", { name: "Retry" }).click();
    await items().filter({ hasText: "Stop the flaky test" }).waitFor({ state: "detached" });
    expect(board().requests.some((r) => r.path === "/v1/cards/6/retry")).toBe(true);
  });

  it.skipIf(usesRealServer)(
    "merges or dismisses a proposal and a takeover, and names a refusal",
    async () => {
      await open("/inbox");
      fake().refuseNext("POST", "/v1/proposals/4/merge", 409, "the branch does not merge cleanly");
      const proposal = item("Let the Coder read the tests first");
      await proposal.getByRole("button", { name: "Merge" }).click();
      await proposal
        .getByRole("alert")
        .filter({ hasText: "the branch does not merge cleanly" })
        .waitFor();
      await proposal.getByRole("button", { name: "Dismiss" }).click();
      await proposal.waitFor({ state: "detached" });
      expect(fake().requests.map((r) => r.path)).toContain("/v1/proposals/4/dismiss");

      await item("Raise the round limit").getByRole("button", { name: "Merge" }).click();
      await item("Raise the round limit").waitFor({ state: "detached" });
      expect(fake().requests.map((r) => r.path)).toContain("/v1/proposals/5/merge");

      await item("The decision model can take over").getByRole("button", { name: "Merge" }).click();
      await item("The decision model can take over").waitFor({ state: "detached" });
      expect(fake().requests.map((r) => r.path)).toContain("/v1/takeovers/2/merge");
    },
  );

  it.skipIf(usesRealServer)(
    "edits the issues of a grilling, takes one out, and confirms the list",
    async () => {
      await open("/inbox");
      const grilling = item("3 issues from a grilling");
      await grilling.getByText("Blocked by issue 1, 2").waitFor();
      await grilling.getByLabel("Title of issue 3").fill("Document the export, renamed");
      // Taking the first out renumbers the others, and nothing waits for it any more.
      await grilling.getByRole("button", { name: "Remove issue 1" }).click();
      await grilling.getByLabel("Title of issue 2").waitFor();
      expect(await grilling.getByLabel("Title of issue 3").count()).toBe(0);
      await grilling.getByText("Blocked by issue 1", { exact: true }).waitFor();
      await grilling.getByText("Blocked by issue 1, 2").waitFor({ state: "detached" });
      // A blank title cannot be confirmed.
      await grilling.getByLabel("Title of issue 1").fill("  ");
      expect(await grilling.getByRole("button", { name: "Confirm" }).isDisabled()).toBe(true);
      await grilling.getByLabel("Title of issue 1").fill("Write the CSV");
      // The list stays while the snapshot is read again.
      await page.evaluate(() => window.dispatchEvent(new Event("focus")));
      await grilling.getByRole("button", { name: "Confirm" }).click();
      await grilling.waitFor({ state: "detached" });
      expect(lastBody(fake().requests, "POST", "/v1/grillings/7/confirm")).toEqual({
        issues: [
          { title: "Write the CSV", body: "Its columns.", blocked_by: [] },
          { title: "Document the export, renamed", body: "In the guide.", blocked_by: [0] },
        ],
      });
    },
  );

  it.skipIf(usesRealServer)(
    "dismisses the report of a grilling, and an item whose issues are all gone",
    async () => {
      await open("/inbox");
      await item("A report from a grilling").getByRole("button", { name: "Dismiss" }).click();
      await item("A report from a grilling").waitFor({ state: "detached" });
      expect(fake().requests.map((r) => r.path)).toContain("/v1/grillings/8/dismiss");

      const grilling = item("3 issues from a grilling");
      for (const number of [3, 2, 1]) {
        await grilling.getByRole("button", { name: `Remove issue ${number}` }).click();
      }
      await grilling.getByText("No issue is left.").waitFor();
      expect(await grilling.getByRole("button", { name: "Confirm" }).isDisabled()).toBe(true);
    },
  );

  it.skipIf(usesRealServer)("opens the live view of a card and the chat of a change", async () => {
    await open("/inbox");
    await item("Speed up the build").getByRole("button", { name: "Open live view" }).click();
    await page.waitForURL("**/board/1?card=5");
    await page.goBack();
    await item("Add a retry helper").getByRole("button", { name: "Open chat" }).click();
    await page.waitForURL("**/chat/9");
  });

  it.skipIf(usesRealServer)("merges the change of a chat", async () => {
    await open("/inbox");
    await item("Add a retry helper").getByRole("button", { name: "Merge" }).click();
    await item("Add a retry helper").waitFor({ state: "detached" });
    expect(fake().requests.map((r) => r.path)).toContain("/v1/changes/9/merge");
  });

  it.skipIf(usesRealServer)("shows an item that arrives while the page is open", async () => {
    await open("/inbox");
    await items().first().waitFor();
    fake().addInboxItem({
      kind: "stuck",
      card_id: 1,
      title: "A new stuck card",
      data: { reason: "no model" },
    });
    server.broadcast?.({ kind: "card_updated", data: {} });
    await item("A new stuck card").waitFor();
    await page.getByRole("button", { name: "Stuck 2" }).waitFor();
  });
});

describe("projects", () => {
  it("adds a repository, opens its board, lists it and removes it", async () => {
    await open("/projects");
    await page.getByRole("heading", { name: "Projects", level: 1 }).waitFor();
    // With no repository yet the empty page offers the link too; the header's is the one every page has.
    await page.locator("header").getByRole("link", { name: "Add a repository" }).click();
    await page.waitForURL("**/projects/new");
    const add = page.getByRole("button", { name: "Add", exact: true });
    expect(await add.isDisabled()).toBe(true);
    await page.getByLabel("Name").fill("fresh");
    await page.getByLabel("URL or path").fill(usesRealServer ? folder() : "/srv/git/fresh");
    await add.click();
    await page.waitForURL(/\/board\/\d+$/);
    await page.getByRole("heading", { name: "fresh", level: 1 }).waitFor();

    await page
      .getByRole("navigation", { name: "Pages" })
      .getByRole("link", { name: "All projects" })
      .click();
    await page.waitForURL("**/projects");
    const row = page.getByTestId("repository").filter({ hasText: "fresh" });
    await row.getByText("No forge · 0 cards").waitFor();
    await row.getByRole("button", { name: "Delete" }).click();
    await row.getByText("Delete fresh with its cards and chats?").waitFor();
    await row.getByRole("button", { name: "Delete" }).click();
    await row.waitFor({ state: "detached" });
  });

  it.skipIf(usesRealServer)(
    "lists the repositories with their forge and cards, and asks before it deletes",
    async () => {
      await open("/projects");
      const demo = page.getByTestId("repository").filter({ hasText: "demo" });
      await demo.getByText("GitHub · acme/demo · 7 cards").waitFor();
      await demo.getByRole("button", { name: "Delete" }).click();
      await demo.getByText("Delete demo with its cards and chats?").waitFor();
      await demo.getByRole("button", { name: "Keep" }).click();
      await demo.getByRole("button", { name: "Delete" }).waitFor();
      expect(await page.getByText("Delete demo with its cards and chats?").count()).toBe(0);
      expect(fake().requests.filter((r) => r.method === "DELETE")).toHaveLength(0);

      // A run that goes on keeps the repository, and the server says so.
      await demo.getByRole("button", { name: "Delete" }).click();
      await demo.getByRole("button", { name: "Delete" }).click();
      await alert("run 1 of demo is still going").waitFor();
      await demo.getByRole("link", { name: /demo/ }).waitFor();
      expect(fake().requests.filter((r) => r.method === "DELETE")).toHaveLength(1);
    },
  );

  it.skipIf(usesRealServer)(
    "opens the board and the settings of a repository from the list",
    async () => {
      await open("/projects");
      const demo = page.getByTestId("repository").filter({ hasText: "demo" });
      await demo.getByRole("link", { name: /demo/ }).first().click();
      await page.waitForURL("**/board/1");
      await page.goBack();
      await page.getByRole("link", { name: "Settings and files" }).click();
      await page.waitForURL("**/projects/1");
      await page.getByRole("heading", { name: "demo", level: 1 }).waitFor();
    },
  );

  it.skipIf(usesRealServer)(
    "asks for the repository on the forge and sends the trusted logins",
    async () => {
      await open("/projects/new");
      const add = page.getByRole("button", { name: "Add", exact: true });
      await page.getByLabel("Name").fill("radiance");
      await page.getByLabel("URL or path").fill("https://github.com/acme/radiance.git");
      // Without a forge the form is complete; with GitHub it needs the repository there.
      expect(await add.isEnabled()).toBe(true);
      await page.getByRole("button", { name: "GitHub", exact: true }).click();
      expect(await add.isDisabled()).toBe(true);
      await page.getByLabel("GitHub repository").fill("acme/radiance");
      await page.getByLabel("Trusted authors").fill("leon, mona");
      await add.click();
      await page.waitForURL(/\/board\/\d+$/);
      expect(lastBody(fake().requests, "POST", "/v1/repositories")).toEqual({
        name: "radiance",
        url: "https://github.com/acme/radiance.git",
        forge: "github",
        github: "acme/radiance",
        trusted: ["leon", "mona"],
      });
    },
  );

  it.skipIf(usesRealServer)("asks for the project of GitLab and of Forgejo", async () => {
    await open("/projects/new");
    await page.getByRole("button", { name: "GitLab", exact: true }).click();
    await page.getByLabel("GitLab project").waitFor();
    await page.getByRole("button", { name: "Forgejo", exact: true }).click();
    await page.getByLabel("Forgejo repository").waitFor();
    expect(await page.getByLabel("GitLab project").count()).toBe(0);
    await page.getByRole("button", { name: "None", exact: true }).click();
    expect(await page.getByLabel("Trusted authors").count()).toBe(0);
  });

  it.skipIf(usesRealServer)(
    "shows the sentence of the server when it refuses a repository",
    async () => {
      await open("/projects/new");
      await page.getByLabel("Name").fill("demo");
      await page.getByLabel("URL or path").fill("/srv/git/demo");
      await page.getByRole("button", { name: "Add", exact: true }).click();
      await alert("a repository named demo exists already").waitFor();
      expect(path()).toBe("/projects/new");
      await page.getByLabel("Name").fill("other");
      await page.getByLabel("URL or path").fill("/srv/git/unreachable");
      await page.getByRole("button", { name: "Add", exact: true }).click();
      await alert("git clone failed: repository not found").waitFor();
    },
  );

  it.skipIf(usesRealServer)("edits the trusted logins of a repository on a forge", async () => {
    await open("/projects/1");
    await page.getByTestId("trusted").getByText("No trusted authors").waitFor();
    await page.getByRole("button", { name: "Edit trusted authors" }).click();
    await page.getByLabel("Trusted authors").fill("leon, mona");
    await page.getByRole("button", { name: "Save" }).click();
    await page.getByTestId("trusted").getByText("Trusts leon, mona").waitFor();
    expect(lastBody(fake().requests, "PUT", "/v1/repositories/1")).toEqual({
      trusted: ["leon", "mona"],
    });

    // A refusal is shown, and the logins stay.
    await page.getByRole("button", { name: "Edit trusted authors" }).click();
    fake().refuseNext("PUT", "/v1/repositories/1", 422, "a login is blank or has other characters");
    await page.getByRole("button", { name: "Save" }).click();
    await alert("a login is blank or has other characters").waitFor();
    await page.getByRole("button", { name: "Cancel" }).click();
    await page.getByTestId("trusted").getByText("Trusts leon, mona").waitFor();
  });

  it.skipIf(usesRealServer)("browses the files, edits one and saves it as a commit", async () => {
    await open("/projects/1");
    const tree = page.getByRole("list", { name: "Files" });
    await tree.getByRole("button", { name: "src" }).click();
    await tree.getByRole("button", { name: "server" }).waitFor();
    await tree.getByRole("button", { name: "main.rs", exact: true }).click();
    const text = page.getByLabel("Text of src/main.rs");
    await text.waitFor();
    expect(await text.inputValue()).toContain("fn main()");
    expect(await page.getByRole("button", { name: "Save" }).isDisabled()).toBe(true);

    await text.fill("fn main() {}\n");
    await page.getByText("Unsaved changes.").waitFor();
    // The server refuses (a protected branch): the text stays unsaved.
    fake().refuseNext(
      "PUT",
      "/v1/repositories/1/files/src/main.rs",
      409,
      "git push --quiet origin main failed: protected branch",
    );
    await page.getByRole("button", { name: "Save" }).click();
    await alert(
      "The server refused: git push --quiet origin main failed: protected branch",
    ).waitFor();
    await page.getByText("Unsaved changes.").waitFor();

    // Ctrl+S saves.
    await text.focus();
    await page.keyboard.press("Control+s");
    await page.getByText("Committed to the default branch.").waitFor();
    expect(lastBody(fake().requests, "PUT", "/v1/repositories/1/files/src/main.rs")).toEqual({
      text: "fn main() {}\n",
    });
    expect(await page.getByText("Unsaved changes.").count()).toBe(0);
    // What was saved is what the file is now.
    expect(await text.inputValue()).toBe("fn main() {}\n");
  });

  it.skipIf(usesRealServer)(
    "keeps the text of a tab while another is shown and asks before it drops it",
    async () => {
      await open("/projects/1");
      const tree = page.getByRole("list", { name: "Files" });
      await tree.getByRole("button", { name: "README.md" }).click();
      await page.getByLabel("Text of README.md").fill("# changed\n");
      await tree.getByRole("button", { name: "docs" }).click();
      await tree.getByRole("button", { name: "guide.md" }).click();
      await page.getByLabel("Text of docs/guide.md").waitFor();
      await page.getByRole("tab", { name: /README\.md/ }).click();
      expect(await page.getByLabel("Text of README.md").inputValue()).toBe("# changed\n");

      await page.getByRole("button", { name: "Close README.md" }).click();
      await page.getByText("README.md has unsaved changes.").waitFor();
      await page.getByRole("button", { name: "Keep" }).click();
      await page.getByText("README.md has unsaved changes.").waitFor({ state: "detached" });
      await page.getByRole("button", { name: "Close README.md" }).click();
      await page.getByRole("button", { name: "Discard" }).click();
      await page.getByRole("tab", { name: /README\.md/ }).waitFor({ state: "detached" });
      // The tab before it is shown.
      await page.getByLabel("Text of docs/guide.md").waitFor();
    },
  );

  it.skipIf(usesRealServer)("says that a binary file or a large one is not edited", async () => {
    await open("/projects/1");
    const tree = page.getByRole("list", { name: "Files" });
    await tree.getByRole("button", { name: "logo.png" }).click();
    await page.getByText("logo.png is binary (48211 bytes), not editable here.").waitFor();
    await tree.getByRole("button", { name: "data.bin" }).click();
    await page.getByText("data.bin is over 1 MB (2000000 bytes), not editable here.").waitFor();
    expect(await page.getByRole("textbox", { name: /^Text of/ }).count()).toBe(0);
  });

  it.skipIf(usesRealServer)(
    "goes to a file by what is typed, with the keyboard or the mouse",
    async () => {
      await open("/projects/1");
      const field = page.getByRole("textbox", { name: "Go to file" });
      const tree = page.getByRole("list", { name: "Files", exact: true });
      // Where the tree sits in the page, whatever the viewport is scrolled to.
      const treeTop = () =>
        tree.evaluate((list) => list.getBoundingClientRect().top + window.scrollY);
      const before = await treeTop();
      await field.fill("mr");
      const matches = page.getByRole("list", { name: "Matching files" });
      const best = matches.getByRole("button").filter({ hasText: /main\.rs$/ });
      await best.first().waitFor();
      expect(await best.first().textContent()).toBe("src/main.rs");
      // The dropdown hangs over the tree rather than shoving it down.
      expect(Math.abs((await treeTop()) - before)).toBeLessThan(1);
      await field.press("ArrowDown");
      await field.press("Enter");
      await page.getByLabel("Text of src/server/main.rs").waitFor();
      // The directories above the file are open in the tree.
      await page
        .getByRole("list", { name: "Files" })
        .getByRole("button", { name: "server" })
        .waitFor();
      // The file is chosen, so the bar empties and the tree is on its own again.
      expect(await field.inputValue()).toBe("");
      expect(await matches.count()).toBe(0);

      // Ctrl+P puts the caret in the bar, and the X empties it again.
      await page.keyboard.press("Control+p");
      expect(await page.evaluate(() => document.activeElement?.getAttribute("aria-label"))).toBe(
        "Go to file",
      );
      await page.keyboard.type("guide");
      await page.getByRole("button", { name: "Clear go to file" }).click();
      expect(await field.inputValue()).toBe("");
      expect(await matches.count()).toBe(0);

      // Typing again and clicking a result opens the file.
      await field.fill("guide");
      await page
        .getByRole("list", { name: "Matching files" })
        .getByRole("button", { name: "docs/guide.md" })
        .click();
      await page.getByLabel("Text of docs/guide.md").waitFor();
    },
  );

  it.skipIf(usesRealServer)("leaves Ctrl+P to the browser while a dialog is open", async () => {
    await open("/projects/1");
    await page.getByRole("heading", { name: "Files" }).waitFor();
    await page.keyboard.press("Control+k");
    await page.getByRole("dialog").waitFor();
    // The shortcut must not be taken (not marked as handled) and the caret must not move behind the scrim.
    const handled = await page.evaluate(
      () =>
        new Promise<boolean>((resolve) => {
          window.addEventListener("keydown", (e) => resolve(e.defaultPrevented), { once: true });
          // Listens after the app's own handler, which is registered earlier on the same target.
          window.dispatchEvent(
            new KeyboardEvent("keydown", { key: "p", ctrlKey: true, cancelable: true }),
          );
        }),
    );
    expect(handled).toBe(false);
    expect(await page.evaluate(() => document.activeElement?.getAttribute("aria-label"))).not.toBe(
      "Go to file",
    );
  });

  it.skipIf(usesRealServer)("searches the files and opens a hit at its line", async () => {
    await open("/projects/1");
    await page.getByRole("heading", { name: "Files" }).waitFor();
    await page.keyboard.press("Control+Shift+F");
    const field = page.getByRole("textbox", { name: "Search in files" });
    expect(await page.evaluate(() => document.activeElement?.getAttribute("aria-label"))).toBe(
      "Search in files",
    );
    await field.fill("main");
    await page.getByText(/^\d+ hits in 3 files$/).waitFor();
    const hits = page.getByRole("list", { name: "Search hits" });
    await hits.getByText("src/server/main.rs", { exact: true }).waitFor();
    await hits.getByRole("button", { name: /mod main_loop;/ }).click();
    const text = page.getByLabel("Text of src/server/main.rs");
    await text.waitFor();
    // The cursor is at the start of line 3.
    await page.waitForFunction(
      () =>
        (document.querySelector("textarea[aria-label^='Text of']") as HTMLTextAreaElement)
          ?.selectionStart === 14,
    );

    // A search that fails says why, and a regular expression is asked for on its own.
    await page.getByLabel("Regex").check();
    await field.fill("(");
    await alert("fatal: unmatched parenthesis").waitFor();

    // Clearing the bar puts the editor back on its own.
    await page.getByRole("button", { name: "Clear search in files" }).click();
    expect(await hits.count()).toBe(0);
  });

  it("opens the page of a repository with its files for a real server", async () => {
    await open("/projects");
    const made = await context.request.post(`${server.url}/v1/repositories`, {
      headers: { "X-Moebius": "1" },
      data: { name: "browse", url: usesRealServer ? folder() : "/srv/git/browse" },
    });
    expect(made.ok(), await made.text()).toBe(true);
    const { id } = (await made.json()) as { id: number };
    await page.goto(`${server.url}/projects/${id}`);
    await page.getByRole("heading", { name: "browse", level: 1 }).waitFor();
    await page.getByRole("heading", { name: "Files" }).waitFor();
    await page.getByRole("textbox", { name: "Go to file" }).waitFor();
    await page.getByText("This repository is on no forge").waitFor();
  });
});

describe("the fleet", () => {
  it("loads for a real server and for the fake", async () => {
    await open("/fleet");
    await page.getByRole("heading", { name: "Fleet", level: 1 }).waitFor();
    await page
      .getByText(usesRealServer ? "Nothing is working." : /agents? working/)
      .first()
      .waitFor();
  });

  it.skipIf(usesRealServer)(
    "shows a tile for every working card and chat, and follows their events",
    async () => {
      await open("/fleet");
      // Chat ids start at 1 as the board's run ids do: an ended chat takes the id of the working card's run, and its
      // events are the ones the card's tile reads.
      server.chat?.seed({
        status: "succeeded",
        title: "Add a rule against vendor edits",
        events: [
          [
            "tool_execution_start",
            {
              type: "tool_execution_start",
              toolCallId: "t1",
              toolName: "bash",
              args: { command: "cargo test" },
            },
          ],
        ],
      });
      const chat = server.chat?.seed({
        status: "running",
        title: "Write a ready.luau that starts a card only when nothing blocks it.",
        repository_id: 1,
      }) as number;
      server.chat?.toolCall(chat, {
        id: "c1",
        tool: "read",
        args: { path: "ready.luau" },
        output: "",
      });
      server.broadcast?.({ kind: "run_started", data: { run: chat } });
      server.broadcast?.({ kind: "lagged", data: { missed: 1 } });

      await page.getByText("2 agents working").waitFor();
      const tiles = page.getByTestId("fleet-tile");
      const card = tiles.filter({ hasText: "#1 Add a feature" });
      await card.getByText("Coder", { exact: true }).waitFor();
      await card.getByText("code", { exact: true }).waitFor();
      await card.getByText("demo · 12k tokens").waitFor();
      await card.getByText("cargo test").waitFor();
      const talk = tiles.filter({ hasText: "Write a ready.luau" });
      await talk.getByText("Moebius", { exact: true }).waitFor();
      await talk.getByText("chat", { exact: true }).waitFor();
      await talk.getByText("demo", { exact: false }).waitFor();
      await talk.getByText("ready.luau", { exact: false }).first().waitFor();
      // A card that is done is not there.
      expect(await tiles.filter({ hasText: "Ship the first release" }).count()).toBe(0);

      // A run event adds a step to the tile of its run.
      expect(await card.getByText("cargo clippy").count()).toBe(0);
      server.chat?.emit(1, "tool_execution_start", {
        type: "tool_execution_start",
        toolCallId: "t2",
        toolName: "bash",
        args: { command: "cargo clippy" },
      });
      await card.getByText("cargo clippy").waitFor();

      // A click opens the live view of the card or the chat.
      await card.click();
      await page.waitForURL("**/board/1?card=1");
      await page.goBack();
      await tiles.filter({ hasText: "Write a ready.luau" }).click();
      await page.waitForURL(`**/chat/${chat}`);
    },
  );

  /**
   * A step of a run is one line in a tile, the way a conversation shows it: a long command is read by its end, so the
   * line keeps that end and fades the beginning it has no room for, rather than turning the step into a paragraph.
   */
  it.skipIf(usesRealServer)(
    "keeps a long command on one line and fades the part that is out of view",
    async () => {
      await open("/fleet");
      const command = `grep -rn "review_conflicts" ${"crates/moebius-core/src/board/".repeat(8)} | head -12`;
      const chat = server.chat?.seed({
        status: "running",
        title: "Look for the review gate",
      }) as number;
      server.chat?.toolCall(chat, { id: "c1", tool: "bash", args: { command }, output: "" });
      const tile = page.getByTestId("fleet-tile").filter({ hasText: "Look for the review gate" });
      const line = tile.getByText("bash", { exact: true }).locator("xpath=following-sibling::*[1]");
      await line.getByText(command, { exact: true }).waitFor();

      // One line of words, faded where it ran out, and the whole command still reachable.
      expect((await line.boundingBox())?.height ?? 0).toBeLessThan(30);
      expect(await line.evaluate((el) => getComputedStyle(el).maskImage)).toContain(
        "linear-gradient",
      );
      expect(await line.getAttribute("title")).toBe(command);
    },
  );

  it.skipIf(usesRealServer)(
    "shows that a question waits for the person, and drops a chat that ended",
    async () => {
      await open("/fleet");
      server.chat?.seed({ status: "succeeded", title: "burn the id of the run" });
      const asking = server.chat?.seed({
        status: "waiting",
        title: "Which branch?",
        asks: true,
      }) as number;
      server.broadcast?.({ kind: "lagged", data: { missed: 1 } });
      const tile = page.getByTestId("fleet-tile").filter({ hasText: "Which branch?" });
      await tile.getByText("asks you").waitFor();
      const chat = server.chat?.chats.find((each) => each.id === asking);
      if (chat) chat.status = "succeeded";
      server.broadcast?.({ kind: "run_ended", data: { run: asking, status: "succeeded" } });
      await tile.waitFor({ state: "detached" });
    },
  );

  it.skipIf(usesRealServer)("is listed in the sidebar with the number working", async () => {
    await open("/fleet");
    await page
      .getByRole("navigation", { name: "Pages" })
      .getByRole("link", { name: /^Fleet\b.*\b1$/ })
      .waitFor();
  });
});

describe("usage", () => {
  it("loads for a real server and for the fake", async () => {
    await open("/usage");
    await page.getByRole("heading", { name: "Usage", level: 1 }).waitFor();
    await page.getByText("Model calls", { exact: true }).waitFor();
    await page.getByRole("button", { name: "Today" }).waitFor();
    await page.getByRole("heading", { name: "Tokens per day" }).waitFor();
    await page.getByRole("heading", { name: "By agent" }).waitFor();
  });

  it.skipIf(usesRealServer)(
    "shows the tiles, the heatmap, the models, the time, the waits, the checks and the agents",
    async () => {
      await open("/usage");
      await page.getByText("1.84M", { exact: true }).first().waitFor();
      // The figure and the budget it is measured against are read together, in that order.
      await page.getByText("of the 8M token budget, 23% used").waitFor();
      await page.getByText("1,284", { exact: true }).waitFor();
      await page.getByText("61%", { exact: true }).first().waitFor();
      await page.getByText("3 h 12 m", { exact: true }).first().waitFor();
      // The heatmap: a cell for each day of 53 weeks, and the busiest day.
      expect(await page.getByTestId("heatmap").locator("span[title]").count()).toBeGreaterThan(300);
      await page.getByText(/busiest day Sat 24 Oct with 2\.52M/).waitFor();
      await page.getByText(/The stronger the colour, the more tokens that day used/).waitFor();
      await page.getByRole("heading", { name: "Tokens by model" }).waitFor();
      const models = page.getByRole("table").first();
      await models.getByText("qwen-flash").waitFor();
      await models.getByText("claude-sonnet").waitFor();
      await page.getByText("1.84M today across 2 models").waitFor();
      await page.getByText("Where the model time goes").waitFor();
      await page.getByText("Answering · 40 calls").waitFor();
      await page.getByText("Longest tool waits").waitFor();
      await page.getByText("Outputs cut from 3.4 MB to 920 KB").waitFor();
      await page.getByRole("heading", { name: "Lint and type checks" }).waitFor();
      await page.getByText("Before the run ends").waitFor();
      await page.getByRole("cell", { name: "Coder" }).waitFor();
    },
  );

  it.skipIf(usesRealServer)("reads another window when it is chosen", async () => {
    await open("/usage");
    await page.getByText("of the 8M token budget, 23% used").waitFor();
    await page.getByRole("button", { name: "7 days" }).click();
    await page.getByText("12.9M in the last 7 days across 2 models").waitFor();
    await page.getByText("1.84M a day on average").waitFor();
    await page.getByRole("button", { name: "30 days" }).click();
    await page.getByText(/in the last 30 days across 2 models/).waitFor();
  });
});

describe("get started", () => {
  it("loads for a real server and for the fake", async () => {
    await open("/start");
    await page.getByRole("heading", { name: "Get started", level: 1 }).waitFor();
    await page.getByRole("heading", { name: "Before the first card" }).waitFor();
    expect(await page.getByTestId("start-step").count()).toBe(5);
  });

  it.skipIf(usesRealServer)("lists the steps with a button for each that is missing", async () => {
    await open("/start");
    await page.getByText("A few things have to be set up before the first card can run.").waitFor();
    const steps = page.getByTestId("start-step");
    await steps
      .filter({ hasText: "Connect a model provider" })
      .getByText("Done", { exact: true })
      .waitFor();
    await steps
      .filter({ hasText: "Install Pi-Bolt" })
      .getByText("To do", { exact: true })
      .waitFor();
    await page
      .getByText("Could not run pi-bolt: it was not found. Set the program on the Settings page.")
      .waitFor();
    // Only a missing step has a button.
    await page.getByRole("link", { name: "Open Settings" }).waitFor();
    for (const name of ["Open Models", "Open Agents", "Add repository"]) {
      expect(await page.getByRole("link", { name }).count()).toBe(0);
    }
    await page.getByRole("link", { name: "Open Settings" }).click();
    await page.waitForURL("**/settings");
  });

  it.skipIf(usesRealServer)("sets up the harness repository and restarts the server", async () => {
    await open("/start");
    fake().refuseNext("POST", "/v1/harness/setup", 409, "harness/ exists and is not empty");
    await page.getByRole("button", { name: "Set up" }).click();
    await alert("harness/ exists and is not empty").waitFor();
    await page.getByRole("button", { name: "Set up" }).click();
    await page
      .getByText("Set up. The server restarts and the page reconnects by itself.")
      .waitFor();
    expect(fake().requests.map((r) => r.path)).toEqual(
      expect.arrayContaining(["/v1/harness/setup", "/v1/server/restart"]),
    );
    await page
      .getByTestId("start-step")
      .filter({ hasText: "Keep agents, skills and MCP servers in a repository" })
      .getByText("Done", { exact: true })
      .waitFor();
  });

  it.skipIf(usesRealServer)(
    "reads the steps again with Refresh, and the sidebar lists the page only while a step is missing",
    async () => {
      await open("/start");
      const pages = page.getByRole("navigation", { name: "Pages" });
      await pages.getByRole("link", { name: "Get started" }).waitFor();
      fake().completeStart();
      await page.getByRole("button", { name: "Refresh" }).click();
      await page.getByText("Everything is set up. Cards can run.").waitFor();
      await pages.getByRole("link", { name: "Get started" }).waitFor({ state: "detached" });
    },
  );

  it.skipIf(usesRealServer)("sends each missing step to the page that fixes it", async () => {
    await open("/start");
    fake().missStep("upstream", "Could not ask https://api.example.com/v1: 401.");
    fake().missStep("coder", "Give the Coder a model on the Agents page.");
    fake().missStep("repository", "No repository is added yet.");
    await page.getByRole("button", { name: "Refresh" }).click();
    await page.getByText("Could not ask https://api.example.com/v1: 401.").waitFor();
    for (const [name, target] of [
      ["Open Models", "/models"],
      ["Open Settings", "/settings"],
      ["Open Agents", "/agents"],
      ["Add repository", "/projects/new"],
    ] as const) {
      await page.getByRole("link", { name }).click();
      await page.waitForURL(`**${target}`);
      await page.goBack();
      await page.getByRole("heading", { name: "Before the first card" }).waitFor();
    }
  });
});

describe("settings", () => {
  it("loads for a real server and for the fake", async () => {
    await open("/settings");
    await page.getByRole("heading", { name: "Settings", level: 1 }).waitFor();
    for (const box of [
      "Server",
      "Board and budgets",
      "Nightly schedule",
      "Harness",
      "Pi-Bolt",
      "GitHub",
      "GitLab",
      "Forgejo",
    ]) {
      // The level keeps the box of a key apart from a webhook of the same forge further down the page.
      await page.getByRole("heading", { name: box, exact: true, level: 2 }).waitFor();
    }
    await page.getByRole("button", { name: "Save" }).waitFor();
    await page.getByRole("heading", { name: "This server" }).waitFor();
    // How the app looks is chosen here rather than in the sidebar, and takes effect at once.
    await page.getByRole("heading", { name: "Appearance", exact: true, level: 2 }).waitFor();
    await page.getByRole("radio", { name: "Light" }).check();
    await expect(page.locator("html").getAttribute("data-theme")).resolves.toBe("light");
    await page.getByRole("radio", { name: "Follow the system" }).check();
  });

  it("saves a key to the file of a real server and reads it back", async () => {
    await open("/settings");
    await page.getByLabel("Coding rounds a card").fill("9");
    await page.getByRole("button", { name: "Save" }).click();
    await page.getByText("Saved.").waitFor();
    await page.reload();
    await page.getByLabel("Coding rounds a card").waitFor();
    expect(await page.getByLabel("Coding rounds a card").inputValue()).toBe("9");
    // A wrong value is refused by the server with its own sentence.
    await page.getByLabel("Runs at a time").fill("0");
    await page.getByRole("button", { name: "Save" }).click();
    await alert(/run_limit|at least|range/i).waitFor();
  });

  it.skipIf(usesRealServer)(
    "shows every key with its value or its default and the file",
    async () => {
      await open("/settings");
      await page.getByRole("heading", { name: "Settings", exact: true }).waitFor();
      expect(await page.getByLabel("Coding rounds a card").inputValue()).toBe("7");
      expect(await page.getByLabel("Tokens a day").inputValue()).toBe("8000000");
      // A key the file does not set shows the default.
      expect(await page.getByLabel("Runs at a time").inputValue()).toBe("");
      await page.getByText("Default 3", { exact: true }).waitFor();
      await page.getByText("Default 127.0.0.1:7171").waitFor();
      expect(await showing(page.getByLabel("Regression gate"))).toBe("true");
      await page.getByText("http://127.0.0.1:7171", { exact: true }).waitFor();
    },
  );

  it.skipIf(usesRealServer)("sends exactly the keys that were changed", async () => {
    await open("/settings");
    await page.getByLabel("Coding rounds a card").waitFor();
    // Nothing changed: Save waits for a change, so nothing can be sent.
    expect(await page.getByRole("button", { name: "Save" }).isDisabled()).toBe(true);
    expect(fake().requests.filter((r) => r.path === "/v1/settings")).toHaveLength(0);

    // A key put back to what the file says is not sent.
    await page.getByLabel("Retro at (HH:MM)").fill("21:00");
    await page.getByLabel("Retro at (HH:MM)").fill("");
    await page.getByLabel("Coding rounds a card").fill("9");
    await page.getByLabel("Tokens a day").fill("");
    await pickOption(page, page.getByLabel("Regression gate"), "false");
    await page.getByLabel("MCP variables").fill("DOCS_TOKEN, OTHER");
    await page.getByRole("button", { name: "Save" }).click();
    await page.getByText("Saved.").waitFor();
    expect(lastBody(fake().requests, "PUT", "/v1/settings")).toEqual({
      set: {
        "board.round_limit": 9,
        "regression.gate": false,
        "harness.mcp_env": ["DOCS_TOKEN", "OTHER"],
      },
      remove: ["budgets.day_tokens"],
    });
    // The page shows what the file says now.
    expect(await page.getByLabel("Coding rounds a card").inputValue()).toBe("9");
    expect(await page.getByLabel("Tokens a day").inputValue()).toBe("");
  });

  it.skipIf(usesRealServer)(
    "takes a table out of the file when all its inputs are blank",
    async () => {
      await open("/settings");
      await page.getByLabel("App id").waitFor();
      await page.getByLabel("App id").fill("12");
      await page.getByLabel("Private key file").first().fill("/k.pem");
      await page.getByRole("button", { name: "Save" }).click();
      await page.getByText("Saved.").waitFor();
      await page.getByLabel("App id").fill("");
      await page.getByLabel("Private key file").first().fill("");
      await page.getByRole("button", { name: "Save" }).click();
      await page.getByText("Saved.").waitFor();
      expect(lastBody(fake().requests, "PUT", "/v1/settings")).toEqual({
        set: {},
        remove: ["github"],
      });
    },
  );

  it.skipIf(usesRealServer)(
    "says what is wrong with a number instead of sending it, and shows the refusal of the server",
    async () => {
      await open("/settings");
      await page.getByLabel("Board tick (ms)").fill("fast");
      await page.getByRole("button", { name: "Save" }).click();
      await alert("Board tick (ms) is a whole number.").waitFor();
      expect(fake().requests.filter((r) => r.path === "/v1/settings")).toHaveLength(0);

      await page.getByLabel("Board tick (ms)").fill("");
      await page.getByLabel("Runs at a time").fill("0");
      await page.getByRole("button", { name: "Save" }).click();
      await alert("run_limit must be at least 1").waitFor();
      // The input keeps what was typed.
      expect(await page.getByLabel("Runs at a time").inputValue()).toBe("0");
    },
  );

  it.skipIf(usesRealServer)(
    "asks for a restart for a key the running server reads only then, and replaces the banner while it restarts",
    async () => {
      await open("/settings");
      await page.getByLabel("Desktop API listens on").fill("127.0.0.1:9999");
      await page.getByRole("button", { name: "Save" }).click();
      await page.getByText("Restart the server to use: listen").waitFor();
      await page.getByRole("button", { name: "Restart" }).click();
      await page.getByText("Restarting the server. The page reconnects by itself.").waitFor();
      expect(await page.getByRole("button", { name: "Restart" }).count()).toBe(0);
      expect(fake().requests.map((r) => r.path)).toContain("/v1/server/restart");
      // When the event stream is back everything is read again, and nothing is left to restart.
      server.broadcast?.({ kind: "lagged", data: { missed: 1 } });
      await page
        .getByText("Restarting the server. The page reconnects by itself.")
        .waitFor({ state: "detached" });
      expect(await page.getByText(/Restart the server to use/).count()).toBe(0);
    },
  );

  it.skipIf(usesRealServer)(
    "shows the webhooks with the secret hidden until asked, and makes a new one",
    async () => {
      await open("/settings");
      const github = page.getByTestId("webhook-github");
      await github.getByText(`${server.url}/v1/webhooks/github`).waitFor();
      await github.getByText("X-Hub-Signature-256").waitFor();
      const secret = "whsec_0123456789abcdef0123456789abcdef";
      expect(await page.getByText(secret).count()).toBe(0);
      await github.getByRole("button", { name: "Show" }).click();
      await github.getByText(secret).waitFor();
      await github.getByRole("button", { name: "Hide" }).click();
      expect(await page.getByText(secret).count()).toBe(0);
      await github.getByRole("button", { name: /^Copy/ }).click();
      await github.getByText("Copied").waitFor();
      expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(secret);

      const gitlab = page.getByTestId("webhook-gitlab");
      await gitlab.getByText("No secret yet: webhooks of this forge are off").waitFor();
      expect(await gitlab.getByRole("button", { name: "Show" }).count()).toBe(0);
      await gitlab.getByRole("button", { name: "Make a secret" }).click();
      await gitlab.getByRole("button", { name: "Make a new secret" }).waitFor();
      expect(fake().requests.map((r) => r.path)).toContain("/v1/webhooks/gitlab/secret");

      // Only a person may make a secret.
      fake().refuseNext("POST", "/v1/webhooks/github/secret", 403, "only a person can do this");
      await github.getByRole("button", { name: "Make a new secret" }).click();
      await alert("only a person can do this").waitFor();
    },
  );
});

describe("deleting a grilling from the inbox", () => {
  it.skipIf(usesRealServer)("asks once, then the grilling leaves the inbox", async () => {
    await open("/inbox");
    const grilling = item("A report from a grilling");
    await grilling.getByRole("button", { name: "Delete" }).click();
    const dialog = page.getByRole("dialog", { name: "Delete this grilling?" });
    await dialog.getByText(/Issues it already filed stay/).waitFor();
    await dialog.getByRole("button", { name: "Keep" }).click();
    await dialog.waitFor({ state: "detached" });
    expect(fake().requests.some((r) => r.method === "DELETE")).toBe(false);
    await grilling.getByRole("button", { name: "Delete" }).click();
    await dialog.getByRole("button", { name: "Delete" }).click();
    await grilling.waitFor({ state: "detached" });
    expect(fake().requests.at(-1)).toMatchObject({ method: "DELETE", path: "/v1/grillings/8" });
  });
});
