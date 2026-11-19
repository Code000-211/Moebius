/**
 * The People page in a real browser: an owner adds a member and hands over the token, the member signs in with it and
 * is not shown the People page, the owner changes the role and disables the member, who is signed out at once; and a
 * person makes and revokes a token of their own. Every wait is on something the page shows.
 */
import { type Browser, chromium, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import { startServer, type TestServer } from "./server";

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

const pages = (page: Page) => page.getByRole("navigation", { name: "Pages" });

/** Adds Lea as a member on the People page of the owner and returns the token that is shown once. */
async function addLea(owner: Page): Promise<string> {
  await owner.goto(`${server.url}/people`);
  await owner.getByTestId("people-page").waitFor();
  await owner.getByLabel("Handle").fill("lea");
  await owner.getByLabel("Name", { exact: true }).fill("Lea");
  await owner.getByRole("button", { name: "Add person" }).click();
  const shown = owner.getByTestId("issued-token").locator("code");
  await shown.waitFor();
  const token = (await shown.textContent()) ?? "";
  expect(token).toMatch(/^moebius-access-[0-9a-f]{64}$/);
  return token;
}

describe("the People page", () => {
  it("adds a member, who signs in with the token and does not see the page, and is signed out when disabled", async () => {
    server = await startServer();
    const ownerContext = await browser.newContext();
    const owner = await ownerContext.newPage();
    owner.setDefaultTimeout(8000);
    await owner.goto(server.loginUrl);
    await pages(owner).waitFor();
    await owner.goto(`${server.url}/tokens`);
    await pages(owner).getByRole("link", { name: "People" }).waitFor();

    const token = await addLea(owner);
    await owner.getByTestId("person-lea").waitFor();

    const memberContext = await browser.newContext();
    const member = await memberContext.newPage();
    member.setDefaultTimeout(8000);
    await member.goto(server.url);
    await member.getByLabel("Token").fill(token);
    await member.getByRole("button", { name: "Sign in" }).click();
    await pages(member).waitFor();
    await member.goto(`${server.url}/tokens`);
    await member.getByTestId("tokens-page").waitFor();
    // The member is shown their own tokens but not the People page, in the sidebar or at its address.
    await pages(member).getByRole("link", { name: "My tokens" }).waitFor();
    expect(await pages(member).getByRole("link", { name: "People" }).count()).toBe(0);
    await member.goto(`${server.url}/people`);
    await member.getByText("There is no such page").waitFor();

    // The owner reads the member's token: used once, by signing in.
    await owner.getByTestId("person-lea").getByRole("button", { name: "Tokens" }).click();
    await owner.getByTestId("tokens-lea").getByText(/^Used/).waitFor();

    // A role is changed in place.
    await owner.getByRole("button", { name: "Role of @lea" }).click();
    await owner.getByRole("menuitemradio", { name: "viewer" }).click();
    await owner.getByRole("button", { name: "Role of @lea" }).getByText("viewer").waitFor();

    // Disabling signs the member out at once: the next thing the member does shows the sign-in page.
    await owner.getByTestId("person-lea").getByRole("button", { name: "Disable" }).click();
    await owner.getByTestId("person-lea").getByText("Disabled").waitFor();
    await member.reload();
    await member.getByRole("button", { name: "Sign in" }).waitFor();
    await memberContext.close();
    await ownerContext.close();
  });

  it("makes a token for oneself, shows it once and revokes it", async () => {
    server = await startServer();
    const context = await browser.newContext();
    const page = await context.newPage();
    await page.goto(server.loginUrl);
    await pages(page).waitFor();
    await page.goto(`${server.url}/tokens`);
    await page.getByLabel("Name").fill("laptop");
    await page.getByRole("button", { name: "Make token" }).click();
    await page
      .getByTestId("issued-token")
      .getByText(/moebius-access-/)
      .waitFor();
    const row = page
      .getByTestId("own-tokens")
      .getByTestId("token-row")
      .filter({ hasText: "laptop" });
    await row.waitFor();
    await row.getByRole("button", { name: "Revoke token laptop" }).click();
    await row.getByText("Revoked").waitFor();
    await context.close();
  });
});
