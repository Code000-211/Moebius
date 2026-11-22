/**
 * Three layout gaps in a real browser: no second hairline under the header of a page with tabs, the project header
 * that stays one 48px row at every width down to the narrow breakpoint, and "Wrap long lines" that makes a long line
 * of code break at the edge of its block instead of scrolling sideways. Every wait is on something the page shows.
 */
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

async function open(path: string, width = 1280) {
  server = await startServer();
  context = await browser.newContext({ viewport: { width, height: 720 } });
  page = await context.newPage();
  page.setDefaultTimeout(8000);
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages", exact: true }).waitFor();
  await page.goto(`${server.url}${path}`);
  await page.locator("[data-page-header]").waitFor();
}

describe("the line under the header", () => {
  it.skipIf(usesRealServer)(
    "is the header's own rule alone: nothing under it draws a second one",
    async () => {
      await open("/agents");
      for (const path of [
        "/agents",
        "/agents/skills",
        "/agents/mcp-servers",
        "/improve/lessons",
        "/improve/experiments",
        "/improve/decisions",
        "/improve",
        "/views",
        "/rules",
        "/fleet",
        "/insights",
        "/projects/1",
        "/projects/1/workflows",
        "/projects/1/cycles",
        "/projects/1/milestones",
        "/projects/1/settings",
      ]) {
        await page.goto(`${server.url}${path}`);
        await page.locator("[data-page-header]").waitFor();
        // Let the page's own content arrive: its first heading or row is drawn.
        await page.waitForLoadState("networkidle");
        const found = await page.evaluate(() => {
          const header = document.querySelector("[data-page-header]") as HTMLElement;
          const bottom = header.getBoundingClientRect().bottom;
          const rules: string[] = [];
          for (const element of Array.from(document.querySelectorAll("main *, #root *"))) {
            const box = element.getBoundingClientRect();
            if (box.width < 200 || box.top < bottom - 1 || box.top > bottom + 48) continue;
            const style = getComputedStyle(element);
            if (parseFloat(style.borderTopWidth) > 0 && style.borderTopStyle !== "none") {
              rules.push(`${element.tagName}.${String(element.className).slice(0, 50)}`);
            }
          }
          return rules;
        });
        expect(found, path).toEqual([]);
      }
    },
  );
});

/** Whether a button of the header shows its words, which a narrow header hides while the button keeps its name. */
const wordsShown = (name: string) =>
  page.getByRole("button", { name }).locator(".header-word").isVisible();

describe("the project header", () => {
  it.skipIf(usesRealServer)(
    "stays one row of 48px, with nothing clipped, from 1280 down to 800px",
    async () => {
      await open("/projects/1");
      for (const width of [1280, 1000, 900, 800]) {
        await page.setViewportSize({ width, height: 720 });
        for (const path of ["/projects/1", "/projects/1/workflows", "/projects/1/milestones"]) {
          await page.goto(`${server.url}${path}`);
          await page.locator("[data-page-header]").waitFor();
          await page.getByRole("button", { name: "Moebius" }).waitFor();
          // Horizontally nothing leaves the header; vertically the tabs' underline may sit on its rule, so a few pixels
          // of give there. The names of what is out are in the message when something is.
          const header = await page.evaluate(() => {
            const bar = document.querySelector("[data-page-header]") as HTMLElement;
            const area = bar.getBoundingClientRect();
            const clipped = Array.from(bar.querySelectorAll("h1, [role=tab], button, a"))
              .filter((each) => {
                const box = each.getBoundingClientRect();
                return (
                  box.width > 0 &&
                  (box.right > area.right + 0.5 ||
                    box.left < area.left - 0.5 ||
                    box.top < area.top - 4 ||
                    box.bottom > area.bottom + 4)
                );
              })
              .map((each) => {
                const box = each.getBoundingClientRect();
                const name = each.textContent || each.getAttribute("aria-label") || each.tagName;
                return `${name} [${Math.round(box.left)},${Math.round(box.top)} to ${Math.round(box.right)},${Math.round(box.bottom)}] in [${Math.round(area.left)},${Math.round(area.top)} to ${Math.round(area.right)},${Math.round(area.bottom)}]`;
              });
            return { height: area.height, clipped, scrolls: bar.scrollWidth > bar.clientWidth };
          });
          expect(header, `${width} ${path}`).toEqual({ height: 48, clipped: [], scrolls: false });
        }
      }
    },
  );

  it.skipIf(usesRealServer)(
    "keeps the tabs, the gear, Moebius and the primary button, and the quiet buttons by their icons",
    async () => {
      await open("/projects/1");
      await page.setViewportSize({ width: 800, height: 720 });
      for (const name of ["Board", "Workflows", "Cycles", "Milestones"]) {
        await page.getByRole("tab", { name }).waitFor();
      }
      await page.getByRole("link", { name: "Project settings" }).waitFor();
      await page.getByRole("button", { name: "More project actions" }).waitFor();
      await page.getByRole("button", { name: "Moebius" }).waitFor();
      await page.getByRole("button", { name: "New issue" }).waitFor();
      // Narrow, the quiet buttons are icons named by their label; wide, they are their words.
      await page.getByRole("button", { name: "Filter" }).waitFor();
      expect(await wordsShown("Filter")).toBe(false);
      await page.setViewportSize({ width: 1280, height: 720 });
      expect(await wordsShown("Filter")).toBe(true);
      expect(await wordsShown("Moebius")).toBe(true);
    },
  );
});

describe("Wrap long lines", () => {
  const LONG = `const long = "${"x".repeat(400)}";`;

  async function answer() {
    await page.goto(`${server.url}/chat`);
    const chat = server.chat;
    if (!chat) throw new Error("these tests run against the fake server");
    chat.autoAnswer = false;
    await page.getByRole("textbox").first().fill("Show code");
    await page.keyboard.press("Enter");
    await page.waitForURL(/\/chat\/\d+$/);
    const run = chat.chats[0]?.id as number;
    chat.agentStart(run);
    chat.delta(run, `Here:\n\n\`\`\`js\n${LONG}\n\`\`\`\n\nEnd.`);
    chat.settle(run);
    await page.getByText("End.", { exact: true }).waitFor();
  }

  const sideways = () =>
    page
      .locator("pre")
      .evaluateAll((all) => all.some((each) => each.scrollWidth > each.clientWidth + 1));

  it.skipIf(usesRealServer)(
    "scrolls a long line sideways at first, breaks it once the switch is on, and keeps the choice",
    async () => {
      await open("/appearance");
      await answer();
      const chatUrl = page.url();
      expect(await sideways()).toBe(true);

      await page.goto(`${server.url}/appearance`);
      const row = page.getByRole("switch", { name: "Wrap long lines" });
      expect(await row.getAttribute("aria-checked")).toBe("false");
      await row.click();
      await expect.poll(() => row.getAttribute("aria-checked")).toBe("true");

      // The choice is the server's, so a reload and the chat see it.
      await page.reload();
      await expect
        .poll(() =>
          page.getByRole("switch", { name: "Wrap long lines" }).getAttribute("aria-checked"),
        )
        .toBe("true");
      await page.goto(chatUrl);
      await page.getByText("End.", { exact: true }).first().waitFor();
      await expect.poll(sideways).toBe(false);
    },
  );
});
