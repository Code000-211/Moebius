/**
 * Where the wiki is kept, in a real browser: an owner of a server with no wiki repository is told the pages are on the
 * server only, opens the dialog, is refused an address the server cannot reach and a repository that has pages while
 * the wiki has, and connects one that works, after which the notice is gone and the Settings page names the repository.
 */
import { type Browser, chromium } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { startServer, type TestServer, usesRealServer } from "./server";

let browser: Browser;
let server: TestServer;

beforeAll(async () => {
  browser = await chromium.launch({
    executablePath: process.env.MOEBIUS_E2E_CHROMIUM || undefined,
  });
});

afterAll(async () => {
  await browser.close();
});

afterEach(async () => {
  await server.stop().catch(() => {});
});

describe("the wiki's repository", () => {
  it.skipIf(usesRealServer)(
    "is connected by an owner from the notice, with the server's refusals told",
    async () => {
      server = await startServer();
      const context = await browser.newContext();
      const page = await context.newPage();
      page.setDefaultTimeout(8000);
      await page.goto(server.loginUrl);
      await page.getByRole("navigation", { name: "Pages" }).waitFor();
      await page.goto(`${server.url}/wiki`);
      await page.getByText("The pages are kept on this server only.").waitFor();

      await page.getByRole("button", { name: "Use a git repository…" }).click();
      const dialog = page.getByRole("dialog", { name: "Use a git repository" });
      const address = dialog.getByLabel("Repository address");
      await address.fill("https://example.com/nowhere.git");
      await dialog.getByRole("button", { name: "Connect" }).click();
      await dialog.getByText("the repository cannot be reached").waitFor();

      // A page, then a repository that has pages of its own: nothing is merged.
      await dialog.getByRole("button", { name: "Cancel" }).click();
      if ("wiki" in server) server.wiki.pushFromElsewhere("Runbook", "# Runbook\n");
      await page.getByRole("button", { name: "Use a git repository…" }).click();
      await address.fill("https://example.com/full.git");
      await dialog.getByRole("button", { name: "Connect" }).click();
      await dialog.getByText("Moebius does not merge them").waitFor();

      await address.fill("https://example.com/wiki.git");
      await dialog.getByRole("button", { name: "Connect" }).click();
      await dialog.waitFor({ state: "hidden" });
      await page.getByText("The pages are kept on this server only.").waitFor({ state: "hidden" });

      await page.goto(`${server.url}/settings`);
      await page.getByText("https://example.com/wiki.git").waitFor();
      expect(await page.getByRole("button", { name: "Use a git repository…" }).count()).toBe(0);
    },
  );
});
