/**
 * Issue templates of kinds and the Jira import in a real browser, against the fake server (which holds the kinds and the
 * Jira site; the real server has neither a Jira to reach nor the fixed cards the board test uses): an owner gives a kind
 * a template on the Kinds page, New issue offers "Use the template" for a kind that has one and the card is made from it,
 * and a project's Import tab runs the import of a Jira project and shows the report with its warnings and what the
 * server refused. Every wait is on something the page shows. The routes are the fake's own (e2e/fake/templates.ts).
 */
import { type Browser, type BrowserContext, chromium, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { pickOption } from "./controls";
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

async function open() {
  server = await startServer();
  context = await browser.newContext({ viewport: { width: 1400, height: 1000 } });
  page = await context.newPage();
  page.setDefaultTimeout(8000);
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
}

/** The fake, which these tests read the requests of. */
function fake() {
  if (!("templates" in server)) throw new Error("these tests run against the fake server");
  return server;
}

describe.skipIf(usesRealServer)("issue templates and the Jira import", () => {
  it("gives a kind a template and makes an issue from it", async () => {
    await open();
    // A kind with a choice, offered by the demo project (id 1).
    const headers = { "x-moebius": "1" };
    const made = await context.request.post(`${server.url}/v1/kinds`, {
      headers,
      data: {
        name: "Bug",
        fields: [{ name: "Severity", type: "choice", options: ["low", "high"] }],
      },
    });
    expect(made.status()).toBe(200);
    await context.request.put(`${server.url}/v1/projects/1/kinds`, {
      headers,
      data: { kinds: ["Bug"] },
    });

    await page.goto(`${server.url}/kinds`);
    const template = page.getByRole("form", { name: "Issue template of Bug" });
    await template.waitFor();
    await template.getByRole("button", { name: "Save template" }).waitFor();
    // Nothing to save while it is empty.
    expect(await template.getByRole("button", { name: "Save template" }).isDisabled()).toBe(true);
    await template.getByLabel("Template title").fill("Bug: what broke");
    await template.getByLabel("Template description").fill("Steps to reproduce:\n\n1. ");
    await pickOption(page, template.getByRole("button", { name: "Template Severity" }), "high");
    await template.getByRole("button", { name: "Save template" }).click();
    await template.getByRole("button", { name: "Delete template" }).waitFor();
    expect(
      fake().templates.calls.find(
        (c) => c.method === "PUT" && c.path === "/v1/templates/issues/Bug",
      )?.body,
    ).toEqual({
      title: "Bug: what broke",
      body: "Steps to reproduce:\n\n1. ",
      fields: { Severity: "high" },
    });

    // New issue: the kind, then the template.
    await page.goto(`${server.url}/board/1`);
    await page.getByRole("button", { name: "New issue" }).click();
    const form = page.getByRole("dialog", { name: "New issue" });
    await pickOption(page, form.getByRole("button", { name: "Kind" }), "Bug");
    const use = form.getByLabel(/Use the template of Bug/);
    await use.check();
    await form.getByRole("button", { name: "Create", exact: true }).click();
    await form.waitFor({ state: "detached" });
    await page.getByText("Bug: what broke").first().waitFor();
    const sent = fake().board.requests.findLast(
      (r) => r.method === "POST" && r.path === "/v1/repositories/1/issues",
    )?.body;
    expect(sent).toMatchObject({ title: "", kind: "Bug", use_template: true });

    // Without the tick the title is needed, as before.
    await page.getByRole("button", { name: "New issue" }).click();
    await pickOption(page, form.getByRole("button", { name: "Kind" }), "Bug");
    expect(await form.getByRole("button", { name: "Create", exact: true }).isDisabled()).toBe(true);
    await form.getByRole("button", { name: "Cancel" }).click();

    // A deleted template is no longer offered.
    await page.goto(`${server.url}/kinds`);
    await template.getByRole("button", { name: "Delete template" }).click();
    await template.getByText("(none yet)").waitFor();
    await page.goto(`${server.url}/board/1`);
    await page.getByRole("button", { name: "New issue" }).click();
    await pickOption(page, form.getByRole("button", { name: "Kind" }), "Bug");
    expect(await form.getByLabel(/Use the template/).count()).toBe(0);
  });

  it("imports a Jira project and shows the report with its warnings", async () => {
    await open();
    await page.goto(`${server.url}/projects/1/settings/import`);
    const form = page.getByRole("form", { name: "Import from Jira" });
    await form.waitFor();
    const run = form.getByRole("button", { name: "Import", exact: true });
    expect(await run.isDisabled()).toBe(true);

    // The server refuses a token it was never given, in its own words.
    await form.getByLabel("Jira address").fill("https://acme.atlassian.net");
    await form.getByLabel("Jira project key").fill("WEB");
    await run.click();
    await page.getByText(/no secret named jira-token/).waitFor();

    // A name that does not start with jira- is not sent at all.
    await form.getByLabel("Secret name").fill("anthropic");
    await form.getByText("The name starts with jira-.").waitFor();
    expect(await run.isDisabled()).toBe(true);
    await form.getByLabel("Secret name").fill("jira-token");

    // A line that is not `user = handle` is named.
    await form.getByLabel("Users who exist already").fill("ada@example.com");
    await form.getByText(/is not Jira user = handle/).waitFor();
    await form.getByLabel("Users who exist already").fill("ada@example.com = ada");

    await form.getByLabel("Email").fill("lead@acme.test");
    await form.getByLabel("API token").fill("s3cret-token");
    await run.click();
    const report = page.getByTestId("jira-report");
    await report.getByText("1 warning").waitFor();
    await report.getByText(/WEB-9: the link to WEB-77 is skipped/).waitFor();
    await report.getByText("Issues").waitFor();
    await report.getByText("3", { exact: true }).first().waitFor();

    // The token was stored once under its name, and the import named the secret and never carried the token.
    const { calls, secrets } = fake().templates;
    expect(secrets["jira-token"]).toBe("s3cret-token");
    const sent = calls.findLast((c) => c.path === "/v1/projects/1/import/jira")?.body;
    expect(sent).toEqual({
      url: "https://acme.atlassian.net",
      project: "WEB",
      token_secret: "jira-token",
      users: "people",
      email: "lead@acme.test",
      user_map: { "ada@example.com": "ada" },
    });
    expect(JSON.stringify(calls)).not.toContain("s3cret-token");
    // The token field is empty again once it was stored.
    expect(await form.getByLabel("API token").inputValue()).toBe("");

    // An address Jira cannot be reached at is said, and no report stands from before.
    await form.getByLabel("Jira address").fill("https://unreachable.example");
    await run.click();
    await page.getByText(/The import failed/).waitFor();
    await page.getByText(/Jira could not be reached/).waitFor();
  });
});
