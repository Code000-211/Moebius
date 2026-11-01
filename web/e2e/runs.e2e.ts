/**
 * The run view, the terminals, the diffs and the Ports page in a real browser. The fake server (e2e/fake/runs.ts) plays
 * the server's side exactly as the reference describes it: run events on the event socket, `POST /v1/runs/{id}/steer`,
 * the diffs, and terminals that speak the WebSocket protocol (bytes both ways, a text resize). Against a real server
 * only the pages that need no data of the fake are tried, and the Terminal page opens a real shell: `echo hi-$((40+2))`
 * must print `hi-42`. Every wait is on something the page shows, never on time.
 */
import { type Browser, type BrowserContext, chromium, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import type { FakeRuns } from "./fake/runs";
import { PASTE_END, PASTE_START } from "./fake/runs";
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

/** A new server and a new browser context that may use the clipboard, signed in and on `path`. */
async function open(path: string) {
  server = await startServer();
  context = await browser.newContext();
  await context.grantPermissions(["clipboard-read", "clipboard-write"], { origin: server.url });
  page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  await page.goto(`${server.url}${path}`);
}

afterEach(async () => {
  await context.close();
  await server.stop().catch(() => {});
});

function fake(): FakeRuns {
  if (!server.runs) throw new Error("these tests run against the fake server");
  return server.runs;
}

const screen = () => page.locator(".xterm-rows");
const terminal = () => page.getByTestId("terminal");
const connected = () => page.locator('[data-testid="terminal"][data-status="open"]');
const tab = (name: string) => page.getByRole("tab", { name });

/** Clicks the terminal, so that the keyboard is in it, and waits until its socket is open. */
async function focusTerminal() {
  await connected().waitFor();
  await terminal().click();
}

/** Waits until the fake has seen `what` typed on its latest terminal. */
async function typed(what: string) {
  await expect.poll(() => fake().terminals.at(-1)?.typed() ?? "").toContain(what);
}

describe("the run view", () => {
  it("says there is no such run", async () => {
    await open("/runs/999999");
    await page.getByRole("heading", { name: "There is no such run" }).waitFor();
  });

  it.skipIf(usesRealServer)("shows the run, its card, and the events as they arrive", async () => {
    await open("/runs/1000");
    await page.getByRole("heading", { name: "Run 1000", level: 1 }).waitFor();
    const info = page.getByTestId("run-info");
    await info.getByText("Running", { exact: true }).waitFor();
    await info.getByText("Coder · code").waitFor();
    const card = info.getByRole("link", { name: /#1 Add a feature/ });
    expect(await card.getAttribute("href")).toBe("/board/1?card=1");

    // What the run did before the page opened, then what it does while the page is open.
    await page.getByText("I am reading the build files.").waitFor();
    fake().emit(1000, "message_update", {
      type: "message_update",
      assistantMessageEvent: {
        type: "text_delta",
        contentIndex: 0,
        delta: " Now I edit the Makefile.",
      },
    });
    await page.getByText("Now I edit the Makefile.").waitFor();
    fake().emit(1000, "tool_check", {
      tool: "bash",
      tool_call_id: "c1",
      decision: "allow",
      reason: "no rule is broken",
    });
    fake().emit(1000, "tool_execution_start", {
      type: "tool_execution_start",
      toolCallId: "c1",
      toolName: "bash",
      args: { command: "make test" },
    });
    await page.getByText("make test").first().waitFor();

    await card.click();
    await page.waitForURL("**/board/1?card=1");
    await page.getByRole("region", { name: "Card inspector" }).waitFor();
  });

  it.skipIf(usesRealServer)("steers a running run", async () => {
    await open("/runs/1000");
    const box = page.getByRole("textbox", { name: "Message" });
    await box.fill("Also update the changelog");
    await box.press("Enter");
    await expect.poll(() => fake().steered.length).toBe(1);
    expect(fake().steered[0]).toEqual({
      run: 1000,
      body: { message: "Also update the changelog" },
    });
    // The server stores a `steered` event, which the conversation shows as the person's message.
    await page.getByText("Also update the changelog").waitFor();
    await expect.poll(() => box.inputValue()).toBe("");
  });

  it.skipIf(usesRealServer)("shows why a steer was refused and keeps the message", async () => {
    await open("/runs/1000");
    fake().refuseSteer = "run 1000 is not running";
    const box = page.getByRole("textbox", { name: "Message" });
    await box.fill("Too late");
    await box.press("Enter");
    await page.getByText("run 1000 is not running").waitFor();
    expect(await box.inputValue()).toBe("Too late");
  });

  it.skipIf(usesRealServer)("cannot steer or open a shell in a run that has ended", async () => {
    await open("/runs/1001");
    await page.getByTestId("run-hint").getByText("This run has ended.").waitFor();
    expect(await tab("Terminal").count()).toBe(0);
    expect(await page.getByRole("textbox", { name: "Message" }).count()).toBe(0);
  });

  it.skipIf(usesRealServer)(
    "has a terminal in the worktree that echoes what is typed",
    async () => {
      await open("/runs/1000");
      await tab("Terminal").click();
      await focusTerminal();
      expect(fake().terminals.at(-1)?.path).toBe("/v1/runs/1000/terminal");
      await page.keyboard.type("echo hello");
      await screen().filter({ hasText: "echo hello" }).waitFor();
      await typed("echo hello");
      await page.keyboard.press("Enter");
      // The shell's answer to the Enter is a new prompt line.
      await expect.poll(() => fake().terminals.at(-1)?.typed()).toBe("echo hello\r");

      // The shell stays open behind the other views.
      await tab("Conversation").click();
      await page.getByText("I am reading the build files.").waitFor();
      await tab("Terminal").click();
      await screen().filter({ hasText: "echo hello" }).waitFor();
      expect(fake().terminals.length).toBe(1);
    },
  );

  it.skipIf(usesRealServer)("shows what the run changed", async () => {
    await open("/runs/1000");
    await tab("Changes").click();
    await page.getByTestId("diff-summary").getByText("2 files").waitFor();
    const added = page.locator('[data-testid="diff-line"][data-kind="added"]');
    const removed = page.locator('[data-testid="diff-line"][data-kind="removed"]');
    await added.filter({ hasText: 'println!("welcome");' }).waitFor();
    await removed.filter({ hasText: 'println!("hello");' }).waitFor();
    expect(await page.getByTestId("diff-path").textContent()).toBe("src/lib.rs");
    await page.getByTestId("diff-file").filter({ hasText: "notes.txt" }).click();
    await added.filter({ hasText: "a note for the reviewer" }).waitFor();
    expect(await removed.count()).toBe(0);

    // Side by side: the old line is left of the new one in the same row.
    await page.getByTestId("diff-file").filter({ hasText: "lib.rs" }).click();
    await page.getByRole("button", { name: "Side by side" }).click();
    const row = page
      .locator("[data-testid=diff-side] .grid")
      .filter({ hasText: 'println!("hello");' });
    await row.locator('[data-kind="removed"]').filter({ hasText: 'println!("hello");' }).waitFor();
    await row.locator('[data-kind="added"]').filter({ hasText: "hello, world" }).waitFor();
  });
});

describe("the diff of a card", () => {
  it.skipIf(usesRealServer)("shows its added and removed lines in the board's dock", async () => {
    await open("/board/1?card=5");
    const dock = page.getByRole("region", { name: "Card inspector" });
    await dock.getByText("Changes", { exact: true }).waitFor();
    await dock
      .locator('[data-testid="diff-line"][data-kind="added"]')
      .filter({ hasText: "cc -O2 main.c" })
      .waitFor();
    await dock
      .locator('[data-testid="diff-line"][data-kind="removed"]')
      .filter({ hasText: "cc -O0 main.c" })
      .waitFor();
    expect(await dock.getByTestId("diff-path").textContent()).toBe("Makefile");
  });
});

describe("the terminals", () => {
  it.skipIf(usesRealServer)("echoes what is typed on the Terminal page", async () => {
    await open("/terminal");
    await focusTerminal();
    expect(fake().terminals.at(-1)?.path).toBe("/v1/terminal");
    await page.keyboard.type("ls -la");
    await screen().filter({ hasText: "ls -la" }).waitFor();
    await typed("ls -la");
    // It is bytes that went over the socket, and nothing the protocol does not know.
    expect(fake().terminals.at(-1)?.invalid).toEqual([]);
  });

  it.runIf(usesRealServer)("runs a command in a real shell", async () => {
    await open("/terminal");
    await focusTerminal();
    await page.keyboard.type("echo hi-$((40+2))");
    await page.keyboard.press("Enter");
    await screen().filter({ hasText: "hi-42" }).waitFor();
  });

  it.skipIf(usesRealServer)(
    "pastes with the right button and with Ctrl+Shift+V, in brackets",
    async () => {
      await open("/terminal");
      await focusTerminal();
      await page.evaluate(() => navigator.clipboard.writeText("first paste"));
      await terminal().click({ button: "right" });
      await typed(`${PASTE_START}first paste${PASTE_END}`);

      await page.evaluate(() => navigator.clipboard.writeText("second paste"));
      await page.keyboard.press("Control+Shift+V");
      await typed(`${PASTE_START}second paste${PASTE_END}`);
      await screen().filter({ hasText: "second paste" }).waitFor();
    },
  );

  it.skipIf(usesRealServer)("copies the selection when the mouse is let go", async () => {
    await open("/terminal");
    await focusTerminal();
    await page.keyboard.type("copyme");
    const word = screen().getByText("copyme");
    await word.waitFor();
    await page.evaluate(() => navigator.clipboard.writeText("before"));
    await word.dblclick({ force: true });
    await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe("copyme");

    // Ctrl+Shift+C copies as well, and sends nothing to the shell.
    await page.evaluate(() => navigator.clipboard.writeText("before again"));
    await page.keyboard.press("Control+Shift+C");
    await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe("copyme");
    expect(fake().terminals.at(-1)?.typed()).toBe("copyme");
  });

  it.skipIf(usesRealServer)(
    "tells the server its size when it opens and when it changes",
    async () => {
      await page?.close().catch(() => {});
      await open("/terminal");
      await connected().waitFor();
      await expect.poll(() => fake().terminals.at(-1)?.resizes.length ?? 0).toBeGreaterThan(0);
      const first = fake().terminals.at(-1)?.resizes.at(-1) as { cols: number; rows: number };
      expect(first.cols).toBeGreaterThan(20);

      await page.setViewportSize({ width: 700, height: 500 });
      await expect
        .poll(() => fake().terminals.at(-1)?.resizes.at(-1)?.cols ?? first.cols)
        .toBeLessThan(first.cols);
      const resizes = fake().terminals.at(-1)?.resizes ?? [];
      expect(resizes.length).toBeGreaterThan(1);
      expect(fake().terminals.at(-1)?.invalid).toEqual([]);
    },
  );

  it.skipIf(usesRealServer)("keeps the board's shell alive while its sheet is closed", async () => {
    await open("/board/1");
    await page.getByRole("button", { name: "Terminal" }).click();
    await connected().waitFor();
    const sheet = page.getByRole("dialog");
    await sheet.waitFor();
    expect(fake().terminals.at(-1)?.path).toBe("/v1/repositories/1/terminal");
    // A sheet too small to read is the reason this one is not a band under the cards.
    const box = await sheet.boundingBox();
    expect(box?.height ?? 0).toBeGreaterThan(300);
    expect(box?.width ?? 0).toBeGreaterThan(600);
    await focusTerminal();
    await page.keyboard.type("keepme");
    await screen().filter({ hasText: "keepme" }).waitFor();

    // Closing the sheet hides the shell rather than ending it: the scrollback, the working directory and
    // whatever is running in it are what a person wants back when they look at a card and come back.
    await sheet.getByRole("button", { name: "Close" }).click();
    await expect.poll(() => page.getByRole("dialog").count()).toBe(0);
    expect(fake().terminals.length).toBe(1);
    expect(fake().terminals.at(-1)?.closed).not.toBe(true);

    // Opening it again is the same shell, and it takes the keyboard without being clicked.
    await page.getByRole("button", { name: "Terminal" }).click();
    await sheet.waitFor();
    expect(fake().terminals.length).toBe(1);
    await screen().filter({ hasText: "keepme" }).waitFor();
    await page.keyboard.type("andagain");
    await typed("andagain");

    // Leaving the board leaves nothing to show the shell, so the socket closes and the server ends it.
    await page.goto(`${server.url}/inbox`);
    await expect.poll(() => fake().terminals.at(-1)?.closed).toBe(true);
  });

  it.skipIf(usesRealServer)(
    "opens Pi-Bolt's terminal interface from a chat that waits",
    async () => {
      await open("/chat");
      const id = server.chat?.seed({
        title: "Waiting",
        events: [["said", { message: "Waiting" }]],
      });
      await page.goto(`${server.url}/chat/${id}`);
      await page.getByTestId("person-message").filter({ hasText: "Waiting" }).waitFor();
      await page.getByRole("button", { name: "Terminal" }).click();
      await connected().waitFor();
      expect(fake().terminals.at(-1)?.path).toBe(`/v1/chats/${id}/tui`);
      await page.getByRole("button", { name: "Conversation" }).click();
      await page.getByTestId("person-message").filter({ hasText: "Waiting" }).waitFor();
      await expect.poll(() => fake().terminals.at(-1)?.closed).toBe(true);
    },
  );

  it.skipIf(usesRealServer)(
    "says only a person can do it when the server refuses a shell",
    async () => {
      await open("/terminal");
      await connected().waitFor();
      await page.close();
      fake().refuseTerminals = 403;
      page = await context.newPage();
      await page.goto(`${server.url}/terminal`);
      const ended = page.getByTestId("terminal-ended");
      await ended.getByText("Only a person can do this").waitFor();
      expect(await ended.getAttribute("role")).toBe("alert");
      expect(fake().terminals.length).toBe(1);

      // Open again, once the server allows it.
      fake().refuseTerminals = null;
      await ended.getByRole("button", { name: "Open again" }).click();
      await connected().waitFor();
    },
  );
});

describe("the Ports page", () => {
  it("opens from the sidebar, behind Settings", async () => {
    await open("/inbox");
    const nav = page.getByRole("navigation", { name: "Pages" });
    await nav.getByRole("link", { name: "Settings" }).click();
    await nav.getByRole("link", { name: "Ports" }).click();
    await page.waitForURL("**/ports");
    await page.getByRole("heading", { name: "Ports", level: 1 }).waitFor();
  });

  it.skipIf(usesRealServer)(
    "lists the ports with whom they belong to, and follows the server",
    async () => {
      await open("/ports");
      const rows = page.getByTestId("port-row");
      await rows.first().waitFor();
      expect(await rows.count()).toBe(2);
      const first = rows.filter({ hasText: "3000" });
      await first.getByText("node").waitFor();
      expect(await first.getByRole("link", { name: "run 1000" }).getAttribute("href")).toBe(
        "/runs/1000",
      );
      await first.getByText("ssh -N -L 3000:127.0.0.1:3000").waitFor();
      await rows.filter({ hasText: "8080" }).getByText("server terminal").waitFor();

      // A port that opens while the page is open shows at once; one that closes goes.
      fake().ports.push({ port: 5173, pid: 7, process: "vite", kind: "chat", run: 12 });
      server.broadcast?.({ kind: "port_opened", data: { port: 5173 } });
      await rows.filter({ hasText: "5173" }).getByRole("link", { name: "chat 12" }).waitFor();
      fake().ports.splice(0, 1);
      server.broadcast?.({ kind: "port_closed", data: { port: 3000 } });
      await expect.poll(() => rows.count()).toBe(2);
      expect(await rows.filter({ hasText: "3000" }).count()).toBe(0);
    },
  );

  it.skipIf(usesRealServer)("says when no port is open", async () => {
    await open("/ports");
    fake().ports.length = 0;
    await page.reload();
    await page.getByTestId("no-ports").waitFor();
  });
});
