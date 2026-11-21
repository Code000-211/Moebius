/**
 * Writes the pictures of the README into `docs/assets/screenshots/`: each page in the dark and the light theme, as
 * `<name>-dark.png` and `<name>-light.png`. It opens the built app in Chromium against the fake server of
 * `../fakeServer.ts`, with the answers of `showcase.ts` in front of it so that the project, its cards, its agents and
 * its inbox look like a real one. `pnpm build && pnpm screenshots` runs it; nothing else does.
 */
import { mkdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { type Browser, chromium, type Page, type Route } from "playwright";
import { afterAll, beforeAll, expect, it } from "vitest";
import { type FakeServer, startFakeServer } from "../fakeServer";
import {
  agentQuestion,
  agentSteps,
  diff,
  diff21,
  events,
  longAnswer,
  repository,
  runSteps,
  snapshot,
  threadOf21,
} from "./showcase";

const OUT =
  process.env.MOEBIUS_SHOT_DIR ?? resolve(import.meta.dirname, "../../../docs/assets/screenshots");
const THEMES = ["dark", "light"] as const;
const CODER_MODEL = "local-coder";

let browser: Browser;
let fake: FakeServer;

beforeAll(async () => {
  browser = await chromium.launch({
    executablePath: process.env.MOEBIUS_E2E_CHROMIUM || undefined,
  });
  fake = await startFakeServer();
  mkdirSync(OUT, { recursive: true });
  await seedRules();
  await seedWiki();
  seedChats();
});

afterAll(async () => {
  await browser?.close();
  await fake?.stop().catch(() => {});
});

async function put(path: string, body: unknown) {
  const answer = await fetch(`${fake.url}${path}`, {
    method: "PUT",
    body: JSON.stringify(body),
    headers: await headers(),
  });
  expect(answer.status).toBe(200);
}

let cookie = "";
async function headers() {
  if (cookie === "") {
    const answer = await fetch(`${fake.url}/v1/session`, {
      method: "POST",
      headers: { Origin: fake.url, "X-Moebius": "1" },
      body: JSON.stringify({ token: fake.token }),
    });
    cookie = (answer.headers.get("set-cookie") ?? "").split(";")[0] ?? "";
  }
  return { cookie, "X-Moebius": "1", Origin: fake.url, "content-type": "application/json" };
}

async function seedRules() {
  await put("/v1/rules", {
    text: `Rules for every run.

## leave-ci-alone
Do not edit files under .github/ or any other CI configuration.

## no-secrets-in-code
Never write an API key, a token or a password into a file. Read it from the environment.

## ask-before-new-dependencies
Do not add a crate to Cargo.toml unless the issue names it.
`,
  });
  await put("/v1/repositories/1/settings/rules.md", {
    text: "## migrations-are-forward-only\nDo not edit a migration that has been merged. Add a new one.",
  });
}

/** The pages of the wiki's picture: a runbook that links to another page and to a card. */
async function seedWiki() {
  await put("/v1/wiki/pages/runbooks/rollback", {
    text: "# Rolling back\n\nRedeploy the tag before the bad one with `deploy.sh <tag>`.\n",
  });
  await put("/v1/wiki/pages/runbooks/deploy", {
    text: `# Deploying the storefront

The storefront deploys from \`main\` every weekday at 10:00. A change waits in the inbox until a person merges it.

## Before

- Every check of the change is green.
- The migrations of the release are forward only; see [[runbooks/rollback|Rolling back]] if one fails.

## Steps

1. Tag the release: \`git tag v$(date +%Y.%m.%d)\`.
2. Run \`deploy.sh\` with the tag, and watch the error rate for ten minutes.
3. Post the tag in the release channel.

The export of orders (SHOP-24) is slow for the bigger shops: deploy it outside the busy hours.
`,
  });
}

/** The chats of the chat page: the one at work is the newest, so it is made last. */
function seedChats() {
  const said = (message: string): [string, Record<string, unknown>] => ["said", { message }];
  const finished = (message: string, answer: string) => {
    const list = events([{ say: answer }]).map((event): [string, Record<string, unknown>] => [
      event.kind,
      event.data,
    ]);
    return [
      said(message),
      ...list,
      ["agent_end", { type: "agent_end", messages: [] }],
      ["agent_settled", { type: "agent_settled", aborted: false }],
    ] as Array<[string, Record<string, unknown>]>;
  };
  const older: Array<[string, string, string, "stopped"]> = [
    [
      "Moebius",
      "Write a rule that keeps agents out of the CI configuration",
      "I added `leave-ci-alone` to the rules.",
      "stopped",
    ],
    [
      "Coder",
      "Why does the orders list scan the whole table?",
      "There is no index on `orders.customer_id`. Card #19 adds one.",
      "stopped",
    ],
    ["Moebius", "Add an issue for the refunds endpoint", "Added #30, blocked by #24.", "stopped"],
    [
      "Coder",
      "Rename `Money::cents` to `Money::minor_units`",
      "Done in 14 files, tests pass.",
      "stopped",
    ],
  ];
  for (const [agent, title, answer, status] of older.reverse()) {
    const id = fake.chat.seed({
      agent,
      title,
      status,
      repository_id: 1,
      model: agent === "Coder" ? CODER_MODEL : null,
      started_at: Math.floor(Date.now() / 1000) - 7_000,
      events: finished(title, answer),
    });
    // Settling an agent's turn makes the chat wait for its person, so the status is set once the events are in.
    const made = fake.chat.chats.find((chat) => chat.id === id);
    if (made) made.status = status;
  }
  // The Moebius agent's earlier conversation: its answer has a code block and a plan that waits to be approved.
  const plan = events(agentSteps, 400).map((event): [string, Record<string, unknown>] => [
    event.kind,
    event.data,
  ]);
  const asked = fake.chat.seed({
    agent: "Moebius",
    title: agentQuestion,
    status: "stopped",
    repository_id: 1,
    origin: "assistant",
    model: null,
    started_at: Math.floor(Date.now() / 1000) - 600,
    events: [
      said(agentQuestion),
      ...plan,
      ["agent_end", { type: "agent_end", messages: [] }],
      ["agent_settled", { type: "agent_settled", aborted: false }],
    ],
  });
  const waiting = fake.chat.chats.find((chat) => chat.id === asked);
  if (waiting) {
    waiting.status = "waiting";
    waiting.asks = true;
  }
  const prompt =
    "The CSV export of orders times out for the bigger shops. Find out why and fix it.";
  const list = events(
    [
      { say: "I will start with the export handler and the query behind it." },
      {
        tool: "bash",
        args: { command: 'rg -n "fn export_orders" src' },
        output: "src/orders/export.rs:18: pub async fn export_orders(",
      },
      {
        tool: "read",
        args: { path: "/srv/.moebius/repos/storefront/src/orders/export.rs" },
        output: "let rows = repo.all_orders(shop).await?;",
      },
      {
        say: "The handler loads every order of the shop into memory before it writes the first line. For a shop with 400,000 orders that is the timeout.",
      },
      {
        tool: "bash",
        args: { command: 'psql -c "EXPLAIN ANALYZE SELECT * FROM orders WHERE shop_id = 7"' },
        output: "Seq Scan on orders  (cost=0.00..48211.00 rows=412093)  Execution Time: 6120.4 ms",
      },
      {
        say: "Streaming the rows in pages of 1,000 keeps memory flat, and the index on `shop_id` makes each page cheap.",
      },
      {
        tool: "edit",
        args: { path: "/srv/.moebius/repos/storefront/src/orders/export.rs" },
        output: "Edited src/orders/export.rs (+27 -9)",
      },
      { tool: "bash", args: { command: "cargo test -p orders export" } },
    ],
    3,
  ).map((event): [string, Record<string, unknown>] => [event.kind, event.data]);
  fake.chat.seed({
    agent: "Coder",
    title: prompt,
    status: "running",
    repository_id: 1,
    model: CODER_MODEL,
    thinking: "medium",
    started_at: Math.floor(Date.now() / 1000) - 260,
    events: [said(prompt), ...list],
  });
}

const json = (route: Route, body: unknown) =>
  route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(body) });

/** Puts the showcase answers in front of the fake server's. */
async function answer(page: Page) {
  await page.route("**/v1/**", async (route) => {
    const url = new URL(route.request().url());
    const path = url.pathname;
    if (route.request().method() !== "GET") return route.fallback();
    if (path === "/v1/snapshot") return json(route, snapshot());
    const run = /^\/v1\/runs\/(10[1234])(\/events)?$/.exec(path);
    if (run) {
      const id = Number(run[1]);
      if (run[2]) {
        const after = Number(url.searchParams.get("after") ?? 0);
        return json(
          route,
          events(runSteps[id] ?? []).filter((event) => event.seq > after),
        );
      }
      const started =
        Math.floor(Date.now() / 1000) -
        (id === 101 ? 410 : id === 102 ? 1_130 : id === 104 ? 1_860 : 250);
      return json(route, {
        id,
        card_id: id === 101 ? 24 : id === 102 ? 26 : id === 104 ? 34 : 22,
        agent: id === 103 ? "Reviewer" : id === 104 ? "Planner" : "Coder",
        harness: "pi-bolt",
        purpose: id === 103 ? "review" : id === 104 ? "plan" : "code",
        status: id === 104 ? "waiting" : "running",
        worktree: `/srv/.moebius/runs/${id}/worktree`,
        started_at: started,
        ended_at: null,
        summary: null,
        repository_id: 1,
        harness_repository: false,
        origin: "chat",
        runner: null,
      });
    }
    if (path === "/v1/changes/1/diff") return json(route, { diff, truncated: false });
    if (path === "/v1/changes/2/diff") return json(route, { diff: diff21, truncated: false });
    if (path === "/v1/cards/21/comments") return json(route, threadOf21.comments);
    if (path === "/v1/cards/21/history") return json(route, threadOf21.history);
    // A card of the showcase is read from the snapshot; what the fake keeps of its own cards is left out.
    const card = /^\/v1\/cards\/(\d+)(?:\/([a-z]+))?$/.exec(path);
    if (card) {
      const found = snapshot()
        .boards.flatMap((board) => board.cards)
        .find((each) => each.id === Number(card[1]));
      if (!card[2] && found) return json(route, found);
      if (card[2] === "history" || card[2] === "comments" || card[2] === "runs")
        return json(route, []);
    }
    // The showcase's repository keeps no `.moebius/` folder, so the board shows no warning about one.
    if (path === "/v1/repositories/1/settings") {
      const response = await route.fetch();
      const body = (await response.json()) as Record<string, unknown>;
      return json(route, { ...body, leftover: [] });
    }
    if (
      path === "/v1/agents" ||
      path === "/v1/models/available" ||
      path === "/v1/rules" ||
      path === "/v1/proposals"
    ) {
      const response = await route.fetch();
      let body = await response.text();
      body = body
        .replaceAll("fake-coder", CODER_MODEL)
        .replaceAll("2026-10-28", "2026-11-04")
        .replaceAll('"demo"', `"${repository.name}"`);
      return route.fulfill({ response, body });
    }
    return route.fallback();
  });
}

async function open(
  theme: (typeof THEMES)[number],
  scale: number,
  height = 900,
  width = 1440,
): Promise<Page> {
  const context = await browser.newContext({
    viewport: { width, height },
    deviceScaleFactor: scale,
    colorScheme: theme,
  });
  const page = await context.newPage();
  const signedIn = await page.request.post(`${fake.url}/v1/session`, {
    headers: { Origin: fake.url, "X-Moebius": "1" },
    data: { token: fake.token },
  });
  expect(signedIn.status()).toBe(204);
  await answer(page);
  return page;
}

/** Opens a page and waits until the network is quiet and the first fade-ins are done. */
async function visit(page: Page, path: string) {
  await page.goto(`${fake.url}${path}`);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  await page.waitForLoadState("networkidle").catch(() => {});
  await page.waitForTimeout(900);
}

/** Asks the Moebius agent something and plays the Moebius agent's answer, with the calls it makes, by hand. */
async function askAssistant(page: Page, text?: string) {
  const before = fake.chat.chats.length;
  fake.chat.autoAnswer = false;
  // The header's button opens the panel, and the reply field sends the message.
  await page.getByRole("button", { name: "Moebius", exact: true }).click();
  await page.getByPlaceholder("Reply to Moebius…").fill("What is waiting on the discount fix?");
  await page.keyboard.press("Enter");
  await expect.poll(() => fake.chat.chats.length).toBe(before + 1);
  const id = (fake.chat.chats[0] as { id: number }).id;
  fake.chat.agentStart(id);
  fake.chat.delta(id, "Let me look at the board.");
  fake.chat.toolCall(id, {
    id: "a",
    tool: "bash",
    args: { command: '"$MOEBIUS" board 1' },
    output: "storefront: 12 cards, 3 agents at work",
  });
  fake.chat.toolCall(id, {
    id: "b",
    tool: "bash",
    args: { command: '"$MOEBIUS" card 30' },
    output: "SHOP-30 Add the refunds endpoint: waiting on SHOP-24",
  });
  if (text) {
    fake.chat.delta(id, `\n\n${text}`);
    fake.chat.settle(id, "Done.");
    await page.waitForTimeout(1200);
    fake.chat.autoAnswer = true;
    return;
  }
  fake.chat.delta(
    id,
    "\n\nOne card: SHOP-30, the refunds endpoint. It waits for SHOP-24, the discount fix, which the Coder is testing now. SHOP-24 is in its first round, so SHOP-30 should start within the hour.",
  );
  fake.chat.settle(id, "Done.");
  await page.waitForTimeout(1200);
  fake.chat.autoAnswer = true;
}

const shoot = (page: Page, name: string, theme: string) =>
  page.screenshot({ path: join(OUT, `${name}-${theme}.png`) });

for (const theme of THEMES) {
  it(`takes the pictures in the ${theme} theme`, async () => {
    const hero = await open(theme, 1.44, 1132, 2012);
    await visit(hero, "/projects/1?card=none");
    await askAssistant(hero);
    await hero.screenshot({ path: join(OUT, `hero-${theme}.png`) });
    // The Moebius agent's conversation is the hero's alone: the lists of the other pages do not hold it.
    for (const [index, chat] of fake.chat.chats.entries())
      if (chat.origin === "assistant") fake.chat.chats.splice(index, 1);
    await hero.context().close();

    const page = await open(theme, 1, 1000);
    // The chat at work is the chat page's picture; the Fleet shows the cards' runs.
    const chatAtWork = fake.chat.chats.find((chat) => chat.status === "running");
    if (chatAtWork) chatAtWork.status = "stopped";
    // A card of the board in its window.
    await visit(page, "/projects/1?card=24");
    await shoot(page, "card", theme);
    await visit(page, "/wiki?page=runbooks/deploy");
    await shoot(page, "wiki", theme);
    await visit(page, "/fleet");
    await shoot(page, "fleet", theme);
    if (chatAtWork) chatAtWork.status = "running";
    await visit(page, "/chat/5");
    await shoot(page, "chat", theme);
    await visit(page, "/rules");
    await page.getByRole("button", { name: "Edit rule no-secrets-in-code" }).click();
    await page.getByRole("dialog").waitFor();
    await page
      .getByLabel("Rule text")
      .fill(
        "Never write an API key, a token or a password into a file. Read it from the environment.\nA test may use a fake value, and says so in its name.",
      );
    await page.waitForTimeout(500);
    await shoot(page, "rules", theme);
    await page.context().close();

    // The inbox is taller, so that its third card ends on the picture.
    const inbox = await open(theme, 1, 1090);
    await visit(inbox, "/inbox");
    await shoot(inbox, "inbox", theme);
    await inbox.context().close();
  });
}

/**
 * The pictures of long text from agents, 1280x720 at twice the size, in both themes: the card window with a long
 * review reason closed and opened, and the agent panel with a long Markdown answer. They go to the folder
 * `MOEBIUS_SHOT_DIR` names, and only when it is set.
 */
for (const theme of THEMES) {
  it.skipIf(!process.env.MOEBIUS_SHOT_DIR)(
    `takes the pictures of long text in the ${theme} theme`,
    async () => {
      const page = await open(theme, 2, 720, 1280);
      await visit(page, "/projects/1?card=21");
      const reason = page.getByRole("button", { name: "Sent back to In progress" });
      await reason.scrollIntoViewIfNeeded();
      await shoot(page, "thread-closed", theme);
      await reason.click();
      await page.waitForTimeout(300);
      await shoot(page, "thread-opened", theme);
      await page.context().close();

      const panel = await open(theme, 2, 720, 1280);
      await visit(panel, "/projects/1?card=none");
      await askAssistant(panel, longAnswer);
      await shoot(panel, "agent-panel", theme);
      await panel.context().close();
    },
  );
}
