/**
 * The guards of `docs/design-web.md` (Motion, Scrolling performance) in a real browser. The main pages are opened
 * against the fake server and the assertion functions of `guards.ts` run on each: no mask-image, no backdrop blur
 * but on a menu, a tooltip or the palette, no animation that never ends, no transition on the size or place of a
 * box, no lift under the pointer, columns that keep their width when a card window opens, and, with 1500 items
 * seeded, fewer than 100 rows drawn in each list. Nothing here measures time: it counts what is drawn and reads
 * computed styles.
 *
 * Every guard also has a negative test: a small page that breaks exactly its rule, on which the guard must throw,
 * so a guard that has gone blind fails here instead of passing for ever.
 */
import { type Browser, type BrowserContext, chromium, type Page } from "playwright";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import {
  fewRows,
  noBackdropBlur,
  noHoverLift,
  noInfiniteAnimation,
  noLayoutTransition,
  noMaskImage,
  quiet,
  widthUnchanged,
} from "./guards";
import { startServer, type TestServer, usesRealServer } from "./server";

let browser: Browser;
let server: TestServer | undefined;
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
  await context?.close();
  await server?.stop().catch(() => {});
  server = undefined;
});

/** Signs in to a fresh fake server and opens `path`. */
async function open(path: string): Promise<TestServer> {
  server = await startServer();
  context = await browser.newContext({ viewport: { width: 1280, height: 720 } });
  page = await context.newPage();
  await page.goto(server.loginUrl);
  await page.getByRole("navigation", { name: "Pages" }).waitFor();
  await page.goto(`${server.url}${path}`);
  return server;
}

/** Opens a page of the fake that is not a rule-breaker, for a guard that has to throw. */
async function fixture(html: string): Promise<void> {
  context = await browser.newContext({ viewport: { width: 1280, height: 720 } });
  page = await context.newPage();
  await page.setContent(`<!doctype html><html><body>${html}</body></html>`);
}

describe.skipIf(usesRealServer)("the quiet rules hold on the main pages", () => {
  it("Inbox", async () => {
    await open("/inbox");
    await page.getByTestId("inbox-item").first().waitFor();
    await quiet(page);
  });

  it("Fleet", async () => {
    await open("/fleet");
    await page.getByTestId("list-row").first().waitFor();
    await quiet(page);
  });

  it("Settings", async () => {
    await open("/settings");
    await page.getByRole("heading", { level: 1 }).first().waitFor();
    await quiet(page);
  });

  it("the board, with the card window open over it and a card running", async () => {
    await open("/projects/1");
    await page.getByTestId("card-2").waitFor();
    await quiet(page);
    await noHoverLift(page, "[data-testid^='card-']");

    // The columns keep their width to the pixel when a card opens in its window, and when it closes.
    await widthUnchanged(page, "section[aria-label]", async () => {
      await page.getByTestId("card-1").click();
      await page.getByRole("dialog", { name: "Card inspector" }).waitFor();
    });
    await quiet(page);
    await widthUnchanged(page, "section[aria-label]", async () => {
      await page.keyboard.press("Escape");
      await page.getByRole("dialog", { name: "Card inspector" }).waitFor({ state: "detached" });
    });
  });
});

describe.skipIf(usesRealServer)("a long list draws only the rows in view", () => {
  it("Inbox, with 1500 items", async () => {
    const fake = await open("/inbox");
    for (let n = 1; n <= 1500; n++) {
      fake.pagesA?.addInboxItem({ kind: "info", title: `Item ${n}` });
    }
    await page.reload();
    await page.getByTestId("inbox-item").first().waitFor();
    await fewRows(page, "[role='listitem']");
  });

  it("Fleet, with 1500 runs", async () => {
    const fake = await open("/inbox");
    for (let n = 1; n <= 1500; n++) fake.chat?.seed({ status: "running", title: `Run ${n}` });
    await page.goto(`${fake.url}/fleet`);
    await page.getByTestId("list-row").first().waitFor();
    await fewRows(page, "[role='listitem']");
  });

  it("a board column, with 1500 cards", async () => {
    const fake = await open("/inbox");
    fake.board?.seedCards(1500, "Backlog");
    await page.goto(`${fake.url}/projects/1`);
    await page.getByTestId("card-2").waitFor();
    await fewRows(page, "section[aria-label='Backlog'] [data-testid^='card-']");
  });
});

describe("each guard throws on a page that breaks its rule", () => {
  it("noMaskImage: an edge fade", async () => {
    await fixture(
      `<div style="overflow:auto;height:50px;mask-image:linear-gradient(black 80%, transparent)"><p style="height:200px">x</p></div>`,
    );
    await expect(noMaskImage(page)).rejects.toThrow(/mask-image/);
  });

  it("noMaskImage: passes on a plain scroller", async () => {
    await fixture(`<div style="overflow:auto;height:50px"><p style="height:200px">x</p></div>`);
    await noMaskImage(page);
  });

  it("noBackdropBlur: a panel that blurs", async () => {
    await fixture(`<div style="backdrop-filter:blur(8px)">x</div>`);
    await expect(noBackdropBlur(page)).rejects.toThrow(/backdrop-filter/);
  });

  it("noBackdropBlur: a menu that blurs more than 16px", async () => {
    await fixture(`<div role="menu" style="backdrop-filter:blur(40px)">x</div>`);
    await expect(noBackdropBlur(page)).rejects.toThrow(/over 16px/);
  });

  it("noBackdropBlur: passes on a small menu", async () => {
    await fixture(`<div role="menu" style="backdrop-filter:blur(16px)">x</div>`);
    await noBackdropBlur(page);
  });

  it("noInfiniteAnimation: a pulse nobody named", async () => {
    await fixture(
      `<style>@keyframes glow{50%{opacity:.5}}</style><div style="animation:glow 1s infinite">x</div>`,
    );
    await expect(noInfiniteAnimation(page)).rejects.toThrow(/forever/);
  });

  it("noInfiniteAnimation: an allowed spinner off screen", async () => {
    await fixture(
      `<style>@keyframes spin{to{opacity:.9}}</style><div style="margin-top:3000px;animation:spin 1s infinite">x</div>`,
    );
    await expect(noInfiniteAnimation(page)).rejects.toThrow(/off screen/);
  });

  it("noInfiniteAnimation: passes on an allowed spinner in view", async () => {
    await fixture(
      `<style>@keyframes spin{to{opacity:.9}}</style><div style="animation:spin 1s infinite">x</div>`,
    );
    await noInfiniteAnimation(page);
  });

  it("noLayoutTransition: a width that eases", async () => {
    await fixture(`<div style="transition:width 100ms">x</div>`);
    await expect(noLayoutTransition(page)).rejects.toThrow(/transitions width/);
  });

  it("noLayoutTransition: transition: all", async () => {
    await fixture(`<div style="transition:all 100ms">x</div>`);
    await expect(noLayoutTransition(page)).rejects.toThrow(/transitions all/);
  });

  it("noLayoutTransition: passes on a colour", async () => {
    await fixture(`<div style="transition:background-color 100ms">x</div>`);
    await noLayoutTransition(page);
  });

  it("noHoverLift: a card that lifts", async () => {
    await fixture(
      `<style>.card{padding:20px;margin:20px}.card:hover{transform:translateY(-4px);box-shadow:0 8px 16px #0004}</style><div class="card">x</div>`,
    );
    await expect(noHoverLift(page, ".card")).rejects.toThrow(/pointer changes/);
  });

  it("noHoverLift: passes on a card whose colour changes", async () => {
    await fixture(
      `<style>.card{padding:20px;margin:20px}.card:hover{background:#eee}</style><div class="card">x</div>`,
    );
    await noHoverLift(page, ".card");
  });

  it("fewRows: a list that draws every row", async () => {
    await fixture(`<ul>${"<li>row</li>".repeat(150)}</ul>`);
    await expect(fewRows(page, "li")).rejects.toThrow(/150 of li/);
  });

  it("fewRows: a list with no row says nothing", async () => {
    await fixture("<ul></ul>");
    await expect(fewRows(page, "li")).rejects.toThrow(/no li/);
  });

  it("fewRows: passes on a windowed list", async () => {
    await fixture(`<ul>${"<li>row</li>".repeat(30)}</ul>`);
    expect(await fewRows(page, "li")).toBe(30);
  });

  it("widthUnchanged: a panel that pushes the columns across", async () => {
    await fixture(
      `<div style="display:flex"><section style="flex:1">a</section><section style="flex:1">b</section><aside id="side" style="display:none;width:300px">c</aside></div>`,
    );
    await expect(
      widthUnchanged(page, "section", async () => {
        await page.evaluate(() => {
          const side = document.getElementById("side");
          if (side) side.style.display = "block";
        });
      }),
    ).rejects.toThrow(/was .* and is/);
  });

  it("widthUnchanged: passes when an overlay opens over them", async () => {
    await fixture(
      `<div style="display:flex"><section style="flex:1">a</section><section style="flex:1">b</section></div><aside id="side" style="display:none;position:fixed;right:0;width:300px">c</aside>`,
    );
    await widthUnchanged(page, "section", async () => {
      await page.evaluate(() => {
        const side = document.getElementById("side");
        if (side) side.style.display = "block";
      });
    });
  });
});
