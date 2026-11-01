/**
 * The pages of Rules, Automations, Retro, Experiments and Decision model, and the script editor, in a real browser:
 * each loads and shows its data, and each action sends its request and shows the outcome (a rule saved or refused, an
 * automation saved, run and deleted, a Retro started, a lesson edited, a research confirmed, an export made, a kind
 * released, a script saved, refused at its line and given up). The fake server (`fake/pages-c.ts`) has the data; against
 * a real server (MOEBIUS_E2E_SERVER) each page's "loads" test runs and so do the script editor's and the tests that make
 * their own data. Every wait is on something the page shows, never on time.
 */
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type Browser, type BrowserContext, chromium, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
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

/** A new server and a browser signed in, wide enough for a list beside a detail. */
async function fresh() {
  server = await startServer();
  context = await browser.newContext({ viewport: { width: 1600, height: 1000 } });
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

const fake = () => {
  if (!server.pagesC) throw new Error("this test needs the fake server");
  return server.pagesC;
};
const chatFake = () => {
  if (!server.chat) throw new Error("this test needs the fake server");
  return server.chat;
};
const lastRequest = () => fake().requests.at(-1);
/** The `text` of the body of the last request. */
const lastText = () => String((lastRequest()?.body as { text?: string } | undefined)?.text);

/** Opens a page by its address and waits for its heading. */
async function open(path: string, title: string) {
  await page.goto(`${server.url}${path}`);
  await page.getByRole("heading", { name: title, level: 1 }).waitFor();
}
const region = (name: string) => page.getByRole("region", { name, exact: true });
const alert = (text: string | RegExp) => page.getByRole("alert").filter({ hasText: text });
const status = (text: string | RegExp) => page.getByRole("status").filter({ hasText: text });

/** The id of a repository to open the script editor of: the fake's, or a new one on a real server. */
async function repository(): Promise<number> {
  if (!usesRealServer) return 1;
  const directory = mkdtempSync(join(tmpdir(), "moebius-e2e-repo-"));
  directories.push(directory);
  const response = await context.request.post(`${server.url}/v1/repositories`, {
    headers: { "X-Moebius": "1" },
    data: { name: "demo", url: directory },
  });
  expect(response.ok(), await response.text()).toBe(true);
  return ((await response.json()) as { id: number }).id;
}

describe("the Rules page", () => {
  it("loads, with the built-in patterns", async () => {
    await fresh();
    await open("/rules", "Rules");
    await region("Built-in patterns").waitFor();
    await region("Built-in patterns").getByText("no-force-push").waitFor();
    await region("Rules in prose").waitFor();
  });

  it.skipIf(usesRealServer)("lists the patterns, the rules and what each refused", async () => {
    await fresh();
    await open("/rules", "Rules");
    const patterns = region("Built-in patterns");
    await patterns.getByText("git push --force rewrites history on the remote").waitFor();
    await patterns.getByText("3 refusals this week").waitFor();
    await patterns.getByText("No refusals this week").waitFor();
    const prose = region("Rules in prose");
    await prose.getByText("rules.md of the harness configuration repository").waitFor();
    await prose.getByText("leave-ci-alone").waitFor();
    await prose.getByText("1 refusal this week").waitFor();
    await prose.getByText("Do not copy dependencies into the tree.").waitFor();
    await region("demo").getByText("keep-migrations").waitFor();
    await region("demo")
      .getByText("Saved as a commit on the default branch (.moebius/rules.md).")
      .waitFor();
  });

  it.skipIf(usesRealServer)("saves a rule edited in its card as the whole file", async () => {
    await fresh();
    await open("/rules", "Rules");
    const prose = region("Rules in prose");
    await prose.getByRole("button", { name: "Edit rule no-vendoring" }).click();
    const text = prose.getByLabel("Rule text");
    await text.fill("Do not copy dependencies into the tree. Use the package manager.");
    await prose.getByRole("button", { name: "Save", exact: true }).click();
    await prose.getByText("Committed: a1b2c3d").waitFor();
    expect(lastRequest()).toMatchObject({
      method: "PUT",
      path: "/v1/rules",
      body: {
        text: "Rules for every run.\n\n## leave-ci-alone\nDo not edit files under .github/ or any other CI configuration.\n\n## no-vendoring\nDo not copy dependencies into the tree. Use the package manager.\n",
      },
    });
    // The card is a card again, with the new text.
    await prose.getByText("Use the package manager.").waitFor();
    expect(await prose.getByLabel("Rule text").count()).toBe(0);
  });

  it.skipIf(usesRealServer)("keeps a refused rule open and says why", async () => {
    await fresh();
    await open("/rules", "Rules");
    const prose = region("Rules in prose");
    await prose.getByRole("button", { name: "Add a rule" }).click();
    const save = prose.getByRole("button", { name: "Save", exact: true });
    await prose.getByLabel("Rule name").fill("two words");
    expect(await save.isDisabled()).toBe(true);
    await prose.getByLabel("Rule name").fill("leave-ci-alone");
    await prose.getByLabel("Rule text").fill("Again.");
    await save.click();
    await alert('line 9: the rule "leave-ci-alone" is named twice').waitFor();
    expect(await prose.getByLabel("Rule name").inputValue()).toBe("leave-ci-alone");
    await prose.getByRole("button", { name: "Cancel" }).click();
    expect(await prose.getByLabel("Rule name").count()).toBe(0);
  });

  it.skipIf(usesRealServer)("adds a rule at the end and deletes one after asking", async () => {
    await fresh();
    await open("/rules", "Rules");
    const prose = region("Rules in prose");
    await prose.getByRole("button", { name: "Add a rule" }).click();
    expect(await prose.getByRole("button", { name: "Delete rule" }).count()).toBe(0);
    await prose.getByLabel("Rule name").fill("ask-first");
    await prose.getByLabel("Rule text").fill("Ask before deleting a branch.");
    await prose.getByRole("button", { name: "Save", exact: true }).click();
    await prose.getByText("ask-first").waitFor();
    expect(lastText()).toMatch(
      /into the tree\.\n\n## ask-first\nAsk before deleting a branch\.\n$/,
    );

    await prose.getByRole("button", { name: "Edit rule leave-ci-alone" }).click();
    await prose.getByRole("button", { name: "Delete rule" }).click();
    await prose.getByText("Delete this rule?").waitFor();
    await prose.getByRole("button", { name: "Keep it" }).click();
    await prose.getByRole("button", { name: "Delete rule" }).waitFor();
    await prose.getByRole("button", { name: "Delete rule" }).click();
    await prose.getByRole("button", { name: "Delete", exact: true }).click();
    await prose.getByText("Committed: a1b2c3d").waitFor();
    expect(await prose.getByText("leave-ci-alone").count()).toBe(0);
    expect(lastText()).not.toContain("leave-ci-alone");
  });

  it.skipIf(usesRealServer)("saves the rules of a repository in its own file", async () => {
    await fresh();
    await open("/rules", "Rules");
    const box = region("demo");
    await box.getByRole("button", { name: "Edit rule keep-migrations" }).click();
    await box.getByLabel("Rule text").fill("Do not edit a migration that exists, ever.");
    await box.getByRole("button", { name: "Save", exact: true }).click();
    await box.getByText("Saved.", { exact: true }).waitFor();
    expect(lastRequest()).toMatchObject({
      method: "PUT",
      path: "/v1/repositories/1/files/.moebius/rules.md",
      body: { text: "## keep-migrations\nDo not edit a migration that exists, ever.\n" },
    });
    // A refusal names the file.
    await box.getByRole("button", { name: "Add a rule" }).click();
    await box.getByLabel("Rule name").fill("keep-migrations");
    await box.getByLabel("Rule text").fill("Twice.");
    await box.getByRole("button", { name: "Save", exact: true }).click();
    await alert('.moebius/rules.md: line 4: the rule "keep-migrations" is named twice').waitFor();
  });

  it.skipIf(usesRealServer)("asks the Moebius agent to write the rules", async () => {
    await fresh();
    await open("/rules", "Rules");
    const prose = region("Rules in prose");
    await prose.getByRole("button", { name: "Let the Moebius agent write them" }).click();
    const ask = prose.getByRole("button", { name: "Ask the agent" });
    expect(await ask.isDisabled()).toBe(true);
    await prose.getByLabel("What should be allowed or forbidden").fill("never edit the lockfile");
    await ask.click();
    await prose.getByRole("link", { name: "Open the chat" }).waitFor();
    const started = chatFake().started.at(-1) as { agent: string; message: string };
    expect(started.agent).toBe("Moebius");
    expect(started.message).toContain('"$MOEBIUS" rules save <file>');
    expect(started.message.endsWith("never edit the lockfile")).toBe(true);
  });
});

describe("the Automations page", () => {
  it("loads, and saves a new automation that can be deleted again", async () => {
    await fresh();
    await open("/automations", "Automations");
    await page.getByRole("button", { name: "New automation" }).click();
    await page.getByLabel("Name of the automation").fill("Weekly cleanup");
    // A save without a prompt says what is missing, and sends nothing.
    await page.getByRole("button", { name: "Save", exact: true }).click();
    await alert("Write the prompt its chats start with.").waitFor();
    expect(await page.getByLabel("Cron, in the server's local time").inputValue()).toBe(
      "0 8 * * *",
    );
    await page.getByLabel("Cron, in the server's local time").fill("15 3 * * 0");
    await page.getByLabel("Prompt").fill("Sum up what the agents did this week.");
    await page.getByRole("button", { name: "Save", exact: true }).click();
    await status("Saved.").waitFor();
    await page.getByRole("button", { name: /Weekly cleanup/ }).waitFor();
    // Saved, it can be fired and deleted.
    await page.getByRole("button", { name: "Delete automation" }).click();
    await page.getByText("Delete this automation?").waitFor();
    await page.getByRole("button", { name: "Delete", exact: true }).click();
    await page.getByRole("button", { name: /Weekly cleanup/ }).waitFor({ state: "detached" });
  });

  it.skipIf(usesRealServer)(
    "lists them, with the editor and the firings of the one chosen",
    async () => {
      await fresh();
      await open("/automations", "Automations");
      const morning = page.getByRole("button", { name: /Morning triage/ });
      await morning.getByText("Weekdays at 8 · next in 3 h").waitFor();
      await page
        .getByRole("button", { name: /Outdated dependencies/ })
        .getByText("Failed")
        .waitFor();
      await page
        .getByRole("button", { name: /Outdated dependencies/ })
        .getByText("the agent Coder cannot run: its model is not served")
        .waitFor();
      await morning.click();
      expect(await page.getByLabel("Name of the automation").inputValue()).toBe("Morning triage");
      expect(await page.getByLabel("Cron, in the server's local time").inputValue()).toBe(
        "0 8 * * 1-5",
      );
      expect(await page.getByLabel("Repeats").inputValue()).toBe("0 8 * * 1-5");
      expect(await page.getByLabel("Works in").inputValue()).toBe("repository:1");
      await page.getByText("Fires next in 3 h.").waitFor();
      const firings = region("Firings");
      await firings.getByText("the chat of the firing before (41) still runs").waitFor();
      const started = firings.getByRole("link", { name: /Chat 41 · succeeded/ });
      expect(await started.getAttribute("href")).toBe("/chat/41");
      // A change of the schedule says that the next firing is not known until it is saved.
      await page.getByLabel("Repeats").selectOption("0 9 * * 5");
      await page.getByText("Save to see when it fires next.").waitFor();
    },
  );

  it.skipIf(usesRealServer)("runs one now and saves a change", async () => {
    await fresh();
    await open("/automations", "Automations");
    await page.getByRole("button", { name: /Morning triage/ }).click();
    await page.getByRole("button", { name: "Run now" }).click();
    await status("Started; its chat is among the firings below.").waitFor();
    expect(lastRequest()).toMatchObject({ method: "POST", path: "/v1/automations/1/run" });
    const link = region("Firings").getByRole("link", { name: /Chat 52 · running/ });
    expect(await link.getAttribute("href")).toBe("/chat/52");

    await page.getByLabel("Name of the automation").fill("Morning triage, early");
    await page.getByLabel("Agent").selectOption("Coder");
    await page.getByRole("button", { name: "Off", exact: true }).click();
    await page.getByRole("button", { name: "Save", exact: true }).click();
    await status("Saved.").waitFor();
    expect(lastRequest()).toMatchObject({
      method: "PUT",
      path: "/v1/automations/1",
      body: {
        name: "Morning triage, early",
        schedule: "0 8 * * 1-5",
        agent: "Coder",
        repository: 1,
        harness: false,
        enabled: false,
      },
    });
    await page.getByText('Off: only "Run now" fires it.').waitFor();
    await page
      .getByRole("button", { name: /Morning triage, early/ })
      .getByText("Off")
      .waitFor();
  });

  it.skipIf(usesRealServer)(
    "shows what the server refuses and what a run could not start",
    async () => {
      await fresh();
      await open("/automations", "Automations");
      await page.getByRole("button", { name: /Morning triage/ }).click();
      fake().refuseNext("PUT", "/v1/automations/1", 422, "the agent Ghost does not exist");
      await page.getByRole("button", { name: "Save", exact: true }).click();
      await alert("the agent Ghost does not exist").waitFor();
      fake().refuseNext("POST", "/v1/automations/1/run", 404, "no such automation");
      await page.getByRole("button", { name: "Run now" }).click();
      await alert("no such automation").waitFor();
      // A schedule with four fields is stopped before it is sent.
      const sent = fake().requests.length;
      await page.getByLabel("Cron, in the server's local time").fill("0 8 * *");
      await page.getByRole("button", { name: "Save", exact: true }).click();
      await alert("A schedule has five fields").waitFor();
      expect(fake().requests.length).toBe(sent);
    },
  );

  it.skipIf(usesRealServer)("follows an automation that fires on its own", async () => {
    await fresh();
    await open("/automations", "Automations");
    await page.getByRole("button", { name: /Morning triage/ }).click();
    await region("Firings").getByText("It has not fired yet.").count();
    await server.broadcast?.({ kind: "automation_fired", data: { automation: 1 } });
    // The page reads the firings again (the fake still has the same two).
    await region("Firings")
      .getByText(/Chat 41/)
      .waitFor();
  });
});

describe("the Retro page", () => {
  it("loads, and runs the Retro of today", async () => {
    await fresh();
    await open("/retro", "Retro");
    await page.getByRole("button", { name: "Run the Retro now" }).click();
    await page.getByText("No errors that day.").waitFor();
    await page
      .getByRole("button", { name: /Reported/ })
      .first()
      .waitFor();
    // The day has its Retro now: a second run is refused, and the page says so.
    await page.getByRole("button", { name: "Run the Retro now" }).click();
    await region("The Retro did not start").waitFor();
  });

  it.skipIf(usesRealServer)(
    "lists the days and shows the errors and proposals of one",
    async () => {
      await fresh();
      await open("/retro", "Retro");
      const latest = page.getByRole("button", { name: /2026-10-28/ });
      await latest.getByText("Proposed").waitFor();
      await latest.getByText("6 errors · 3 proposals").waitFor();
      await page
        .getByRole("button", { name: /2026-10-26/ })
        .getByText("Failed")
        .waitFor();
      const errors = region("Errors of 2026-10-28");
      await errors.getByText("Failed tool calls · 1").waitFor();
      await errors.getByText("bash: cargo test exited with 101").waitFor();
      await errors.getByText("card 48 · run 12 · Coder").first().waitFor();
      await errors.getByText("Refused tool calls · 1").waitFor();
      await errors.getByText("Failed checks · 1").waitFor();
      await region("Left out")
        .getByText(/proposal\/2026-10-28-code is left out/)
        .waitFor();
      const proposals = region("Proposals");
      const first = proposals
        .getByRole("article")
        .filter({ hasText: "Tell the Coder to run the tests first" });
      await first.getByText("Positive").first().waitFor();
      await first.getByText("proposal/2026-10-28-small-commits").waitFor();
      await first.getByText("Cards 48, 50").waitFor();
      await first.getByText("Card 48").waitFor();
      await first.getByText("the run succeeded, run_failed did not happen again").waitFor();
      const settings = proposals.getByRole("article").filter({ hasText: "Raise the round limit" });
      await settings.getByText("moebius.toml", { exact: true }).waitFor();
      await settings.getByText("Untried").waitFor();
      await settings.getByText("No card", { exact: true }).waitFor();
      await settings.getByText("+round_limit = 8").waitFor();
      // Another day.
      await page.getByRole("button", { name: /2026-10-27/ }).click();
      await page.getByText("No errors that day.").waitFor();
      await page.getByText("The Retro proposed nothing.").waitFor();
    },
  );

  it.skipIf(usesRealServer)("says why a second Retro of a day did not start", async () => {
    await fresh();
    await open("/retro", "Retro");
    await page.getByRole("button", { name: "Run the Retro now" }).click();
    await page
      .getByRole("button", { name: /Reported/ })
      .first()
      .waitFor();
    await page.getByRole("button", { name: "Run the Retro now" }).click();
    await alert(/exists already/).waitFor();
  });

  it.skipIf(usesRealServer)("edits a lesson in place and deletes one after asking", async () => {
    await fresh();
    await open("/retro", "Retro");
    const lessons = region("Lessons");
    await lessons.getByText("Do: Add a test with emoji before you change the tokenizer.").waitFor();
    await lessons.getByText("Cards 48, 50 · given 4 times").waitFor();
    await lessons.getByText("Retired", { exact: true }).waitFor();
    await lessons.getByText("Cards 41 · given 0 times · not retrieved for 60 days").waitFor();
    // A retired lesson cannot be edited.
    expect(await lessons.getByRole("button", { name: /Edit the lesson Running/ }).count()).toBe(0);

    await lessons.getByRole("button", { name: "Edit the lesson Changing the tokenizer" }).click();
    await lessons.getByLabel("What to do").fill("");
    expect(await lessons.getByRole("button", { name: "Save", exact: true }).isDisabled()).toBe(
      true,
    );
    await lessons.getByLabel("What to do").fill("Always add a test with emoji.");
    await lessons.getByRole("button", { name: "Save", exact: true }).click();
    await lessons.getByText("Do: Always add a test with emoji.").waitFor();
    expect(lastRequest()).toMatchObject({
      method: "PUT",
      path: "/v1/lessons/3",
      body: { advice: "Always add a test with emoji." },
    });

    await lessons
      .getByRole("button", { name: "Delete the lesson Running the whole test suite" })
      .click();
    await lessons.getByText("Delete this lesson?").waitFor();
    await lessons.getByRole("button", { name: "Keep" }).click();
    await lessons
      .getByRole("button", { name: "Delete the lesson Running the whole test suite" })
      .click();
    await lessons.getByRole("button", { name: "Delete lesson" }).click();
    await lessons.getByText("Running the whole test suite").waitFor({ state: "detached" });
    expect(lastRequest()).toMatchObject({ method: "DELETE", path: "/v1/lessons/1" });
  });
});

describe("the Experiments page", () => {
  it("loads, and stores a brief as a draft", async () => {
    await fresh();
    const id = await repository();
    await open("/experiments", "Experiments");
    await page.getByRole("button", { name: "New brief" }).click();
    const store = page.getByRole("button", { name: "Store the brief" });
    expect(await store.isDisabled()).toBe(true);
    await page.getByLabel("Goal").fill("Make the tests faster");
    await page.getByLabel("Measure").fill("echo 12");
    await page.getByRole("button", { name: "Higher is better" }).click();
    expect(await page.getByLabel("Token budget").inputValue()).toBe("200000");
    await store.click();
    const card = page.getByRole("button", { name: /Make the tests faster/ });
    await card.getByText("Draft").waitFor();
    await region("Brief").getByText("Make the tests faster").waitFor();
    await region("Brief")
      .getByText(/Higher is better · 0 of 200000 tokens used/)
      .waitFor();
    await region("Baseline").getByText("Not measured yet.").waitFor();
    await region("Experiments").getByText("No lever has been tried yet.").waitFor();
    expect(id).toBeGreaterThan(0);
  });

  it.skipIf(usesRealServer)(
    "lists the research and shows the baseline and levers of one",
    async () => {
      await fresh();
      await open("/experiments", "Experiments");
      // The newest is a draft; the finished one is chosen to see its levers.
      await page
        .getByRole("button", { name: /Make the host build faster/ })
        .getByText("Draft")
        .waitFor();
      const done = page.getByRole("button", { name: /Cut the clean build time of the host/ });
      await done.getByText("demo · 3 experiments · 1 kept").waitFor();
      await done.click();
      const brief = region("Brief");
      await brief.getByText("./bench/build.sh").waitFor();
      await brief.getByText("demo · Lower is better · 211400 of 500000 tokens used").waitFor();
      await region("Baseline").getByText("372", { exact: true }).waitFor();
      const levers = region("Experiments");
      const kept = levers.getByRole("row").filter({ hasText: "Split the host crate in two" });
      await kept.getByText("research/1-split-crates").waitFor();
      await kept.getByText("301", { exact: true }).waitFor();
      await kept.getByText("-19.1 %", { exact: true }).waitFor();
      await kept.getByText("Kept", { exact: true }).waitFor();
      await levers
        .getByRole("row")
        .filter({ hasText: "Build with more jobs" })
        .getByText("Not kept")
        .waitFor();
      const failed = levers.getByRole("row").filter({ hasText: "Link with lld" });
      await failed.getByText("Failed", { exact: true }).waitFor();
      await failed.getByText("—").first().waitFor();
      // A finished research has no buttons.
      expect(await brief.getByRole("button").count()).toBe(0);
    },
  );

  it.skipIf(usesRealServer)("confirms a draft, starts it and stops it", async () => {
    await fresh();
    await open("/experiments", "Experiments");
    const brief = region("Brief");
    await brief.getByText("Make the host build faster").waitFor();
    await brief.getByRole("button", { name: "Confirm" }).click();
    await brief.getByText("Confirmed", { exact: true }).waitFor();
    expect(lastRequest()).toMatchObject({ method: "POST", path: "/v1/research/2/confirm" });
    expect(await brief.getByRole("button", { name: "Confirm" }).count()).toBe(0);
    await brief.getByRole("button", { name: "Start now" }).click();
    await brief.getByText("Running", { exact: true }).waitFor();
    expect(lastRequest()).toMatchObject({ method: "POST", path: "/v1/research/2/start" });
    await brief.getByRole("button", { name: "Stop", exact: true }).click();
    await brief.getByText("Stopped", { exact: true }).waitFor();
    expect(lastRequest()).toMatchObject({ method: "POST", path: "/v1/research/2/stop" });
    expect(await brief.getByRole("button").count()).toBe(0);
  });

  it.skipIf(usesRealServer)("edits the brief of a draft in place", async () => {
    await fresh();
    await open("/experiments", "Experiments");
    const brief = region("Brief");
    await brief.getByRole("button", { name: "Edit the brief" }).click();
    await brief.getByLabel("Goal").fill("Make the release build faster");
    await brief.getByLabel("Token budget").fill("0");
    expect(await brief.getByRole("button", { name: "Save", exact: true }).isDisabled()).toBe(true);
    await brief.getByLabel("Token budget").fill("300000");
    await brief.getByRole("button", { name: "Save", exact: true }).click();
    await brief.getByText("Make the release build faster").waitFor();
    expect(lastRequest()).toMatchObject({
      method: "PUT",
      path: "/v1/research/2",
      body: {
        goal: "Make the release build faster",
        measure: "cargo build --release 2>&1 | tail -1",
        better: "lower",
        token_budget: 300000,
      },
    });
    await brief.getByText(/0 of 300000 tokens used/).waitFor();
  });

  it.skipIf(usesRealServer)("shows what the server refuses", async () => {
    await fresh();
    await open("/experiments", "Experiments");
    fake().refuseNext("POST", "/v1/research/2/confirm", 409, "the research is running");
    await region("Brief").getByRole("button", { name: "Confirm" }).click();
    await region("The request was refused").getByText("the research is running").waitFor();
  });
});

describe("the Decision model page", () => {
  it("loads, with nothing decided on a new server", async () => {
    await fresh();
    await open("/decision-model", "Decision model");
    await region("Kinds of decision").waitFor();
    await region("Exports").waitFor();
    await region("Shadowing").waitFor();
    await page.getByRole("button", { name: "Export training set" }).waitFor();
  });

  it.skipIf(!usesRealServer)("says that a new server has no decision model", async () => {
    await fresh();
    await open("/decision-model", "Decision model");
    await page.getByText("Nothing has been decided yet.").waitFor();
    await page.getByText("No export has been made yet.").waitFor();
    await region("Shadowing").getByText("No decision model is configured.").waitFor();
  });

  it.skipIf(usesRealServer)("counts the kinds, lists the exports and the shadowing", async () => {
    await fresh();
    await open("/decision-model", "Decision model");
    await page
      .getByText("1227 judgements in 3 kinds; they leave the server only in an export you start.")
      .waitFor();
    const kinds = region("Kinds of decision");
    const merge = kinds.getByRole("row").filter({ hasText: "merge" });
    await merge.getByText("GET /v1/exports/1/merge.jsonl").waitFor();
    await kinds
      .getByRole("row")
      .filter({ hasText: "approve_issue" })
      .getByText("Not exported yet")
      .waitFor();
    await kinds.getByText("moebius export --out <directory>").waitFor();
    await region("Exports")
      .getByRole("row")
      .filter({ hasText: "#1" })
      .getByText("2026-10-26 07:33 UTC")
      .waitFor();

    const shadowing = region("Shadowing");
    await shadowing
      .getByText(
        /moebius-decider decides alongside the current decider on judgements held out of training\. A kind is proposed in the inbox at 100 of them with at least 95% agreement\./,
      )
      .waitFor();
    await shadowing.getByText("No shadow yet").waitFor();
    await shadowing.getByText("40, 3 failed, 2 dropped").waitFor();
    await shadowing.getByText("85% (needs 95%)").waitFor();
    await shadowing.getByText("97.5% (needs 95%)").waitFor();
    await shadowing.getByText("The decision model", { exact: true }).waitFor();
    await shadowing.getByText("As before").first().waitFor();
    await shadowing
      .getByText("The decision model decides tool_call: it agreed on 97% of 100 judgements.")
      .waitFor();
  });

  it.skipIf(usesRealServer)("exports the training set and lists the new export", async () => {
    await fresh();
    await open("/decision-model", "Decision model");
    await page.getByRole("button", { name: "Export training set" }).click();
    await region("Exports").getByRole("row").filter({ hasText: "#2" }).waitFor();
    expect(lastRequest()).toMatchObject({ method: "POST", path: "/v1/exports" });
    // Every kind has a file now.
    await region("Kinds of decision").getByText("GET /v1/exports/2/tool_call.jsonl").waitFor();
    expect(await page.getByText("Not exported yet").count()).toBe(0);
  });

  it.skipIf(usesRealServer)("says that an export was refused", async () => {
    await fresh();
    await open("/decision-model", "Decision model");
    fake().refuseNext("POST", "/v1/exports", 422, 'no judgements of kind "x"');
    await page.getByRole("button", { name: "Export training set" }).click();
    await region("The export was refused").getByText('no judgements of kind "x"').waitFor();
  });

  it.skipIf(usesRealServer)(
    "releases a kind the model took over, or shows the server's sentence",
    async () => {
      await fresh();
      await open("/decision-model", "Decision model");
      fake().refuseNext(
        "POST",
        "/v1/takeovers/3/release",
        403,
        "only a person can release a takeover",
      );
      await page.getByRole("button", { name: "Release tool_call" }).click();
      await alert("only a person can release a takeover").waitFor();
      await page.getByRole("button", { name: "Release tool_call" }).click();
      await page.getByRole("button", { name: "Release tool_call" }).waitFor({ state: "detached" });
      expect(lastRequest()).toMatchObject({ method: "POST", path: "/v1/takeovers/3/release" });
      await page.getByText("As before").first().waitFor();
    },
  );
});

describe("the script editor", () => {
  /** The editor of the script of a column, once it is open. */
  async function openScript(column: string) {
    const id = await repository();
    await page.goto(`${server.url}/board/${id}/scripts/${encodeURIComponent(column)}`);
    await page.getByRole("heading", { name: `Script of ${column}`, level: 1 }).waitFor();
    const editor = page.getByRole("textbox", { name: `Script of ${column}` });
    await editor.waitFor();
    return { id, editor };
  }
  /** Replaces the text of the editor. */
  async function type(editor: ReturnType<Page["getByRole"]>, text: string) {
    await editor.click();
    await page.keyboard.press("ControlOrMeta+A");
    await page.keyboard.insertText(text);
  }

  it("opens the built-in script, saves one of its own, and goes back to the built-in", async () => {
    await fresh();
    const { editor } = await openScript("Ready");
    await page.getByText("built in", { exact: false }).first().waitFor();
    expect(await page.getByRole("button", { name: "Use the built-in script" }).count()).toBe(0);
    const save = page.getByRole("button", { name: "Save", exact: true });

    await type(editor, '-- my own\nfunction on_card(card)\n  moebius.wait("nothing to do")\nend\n');
    await page.getByRole("button", { name: "Revert changes" }).waitFor();
    await save.click();
    await status("Saved as a commit on the default branch.").waitFor();
    await page.getByText("this repository's own").waitFor();
    // It stays after a reload.
    await page.reload();
    await page.getByText("-- my own").waitFor();
    await page.getByText("this repository's own").waitFor();

    await page.getByRole("button", { name: "Use the built-in script" }).click();
    await page.getByText("Use the built-in script instead?").waitFor();
    await page.getByRole("button", { name: "Keep it" }).click();
    await page.getByRole("button", { name: "Use the built-in script" }).click();
    await page.getByRole("button", { name: "Use the built-in script" }).last().click();
    await status("The column runs the built-in script again.").waitFor();
    await page.getByText("built in", { exact: false }).first().waitFor();
    expect(await page.getByText("-- my own").count()).toBe(0);
  });

  it("refuses a bad script and shows the check's error at its line", async () => {
    await fresh();
    const { editor } = await openScript("Ready");
    await type(editor, 'moebius.wait("fine")\nmoebius.move(card.id,\nmoebius.wait("a"');
    await page.getByRole("button", { name: "Save", exact: true }).click();
    const problem = page.getByTestId("script-problem");
    await problem.waitFor();
    expect(await problem.textContent()).toMatch(/^Line \d+: /);
    // The line is marked in the editor, and the editor keeps what was typed.
    await page.locator(".cm-lint-marker-error").first().waitFor();
    await page.locator(".cm-lintRange-error").first().waitFor();
    expect(await page.locator(".cm-content").textContent()).toContain("moebius.move(card.id,");
    // The script was not saved: it is still the built-in one.
    await page.reload();
    await page.getByText("built in", { exact: false }).first().waitFor();
    await page.getByRole("textbox", { name: "Script of Ready" }).waitFor();
    expect(await page.getByText("moebius.move(card.id,").count()).toBe(0);
  });

  it.skipIf(usesRealServer)(
    "marks the line the fake names, and clears the mark on typing",
    async () => {
      await fresh();
      const { editor } = await openScript("In progress");
      await type(editor, "local a = 1\nlocal b = !!\n");
      await page.getByRole("button", { name: "Save", exact: true }).click();
      await page
        .getByTestId("script-problem")
        .getByText("Line 2: syntax error: Unexpected '!'")
        .waitFor();
      await editor.click();
      await page.keyboard.type("x");
      await page.getByTestId("script-problem").waitFor({ state: "detached" });
      await page.getByRole("button", { name: "Revert changes" }).click();
      await page.getByText("-- The In progress column").waitFor();
    },
  );

  it.skipIf(usesRealServer)("shows what the server refuses besides a bad script", async () => {
    await fresh();
    const { editor } = await openScript("Ready");
    await type(editor, "local a = 1\n");
    fake().refuseNext(
      "PUT",
      "/v1/repositories/1/scripts/Ready",
      409,
      "the push was refused: not a fast-forward",
    );
    await page.getByRole("button", { name: "Save", exact: true }).click();
    await alert("the push was refused: not a fast-forward").waitFor();
    expect(await page.getByTestId("script-problem").count()).toBe(0);
  });
});
