/**
 * The list of rows in a real browser: the fixture page of `fixtures/list.tsx` is built with Vite into a folder of its
 * own and served from memory, and 2000 rows on it must be few in the DOM, 36px high each, with no shadow, and the last
 * one must be reachable.
 */
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { createServer, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { type Browser, chromium, type Page } from "playwright";
import { build } from "vite";
import { afterAll, beforeAll, describe, expect, it } from "vitest";

const WEB = resolve(import.meta.dirname, "..");

let browser: Browser;
let server: Server;
let url: string;
let out: string;

beforeAll(async () => {
  out = mkdtempSync(join(tmpdir(), "moebius-list-"));
  await build({
    root: WEB,
    configFile: false,
    logLevel: "error",
    plugins: [react(), tailwindcss()],
    build: {
      outDir: out,
      emptyOutDir: true,
      rollupOptions: { input: resolve(WEB, "e2e/fixtures/list.html") },
    },
  });
  server = createServer((request, response) => {
    const path = new URL(request.url ?? "/", "http://x").pathname;
    const file = join(out, path === "/" ? "e2e/fixtures/list.html" : path);
    try {
      const body = readFileSync(file);
      const type = file.endsWith(".js")
        ? "text/javascript"
        : file.endsWith(".css")
          ? "text/css"
          : file.endsWith(".woff2")
            ? "font/woff2"
            : "text/html";
      response.writeHead(200, { "content-type": type });
      response.end(body);
    } catch {
      response.writeHead(404).end();
    }
  });
  await new Promise<void>((done) => server.listen(0, "127.0.0.1", done));
  url = `http://127.0.0.1:${(server.address() as AddressInfo).port}/`;
  browser = await chromium.launch({
    executablePath: process.env.MOEBIUS_E2E_CHROMIUM || undefined,
  });
});

afterAll(async () => {
  await browser?.close();
  server?.close();
  if (out) rmSync(out, { recursive: true, force: true });
});

async function open(theme: "light" | "dark" = "light"): Promise<Page> {
  const page = await browser.newPage({
    viewport: { width: 1280, height: 720 },
    colorScheme: theme,
  });
  await page.goto(url);
  await page.getByTestId("list-row").first().waitFor();
  return page;
}

describe("2000 rows in a virtual list", () => {
  it("draws few of them, 36px high, without a shadow", async () => {
    const page = await open();
    const rows = page.getByTestId("list-row");
    await expect.poll(() => rows.count()).toBeGreaterThan(0);
    await expect.poll(() => rows.count()).toBeLessThan(100);
    const first = rows.first();
    const box = await first.boundingBox();
    expect(box?.height).toBe(36);
    expect(await first.evaluate((el) => getComputedStyle(el).boxShadow)).toBe("none");
    // The selected row has its edge drawn without a shadow too.
    const selected = page.locator("[data-selected=true]");
    expect(await selected.evaluate((el) => getComputedStyle(el).boxShadow)).toBe("none");
    const group = await page.getByTestId("group-row").first().boundingBox();
    expect(group?.height).toBe(30);
    await page.close();
  });

  it("reaches the last row with the End key, and still draws few", async () => {
    const page = await open();
    await page.getByRole("list", { name: "Rows" }).focus();
    await page.keyboard.press("End");
    await page.getByText("Row number 1999 with").waitFor();
    await expect.poll(() => page.getByTestId("list-row").count()).toBeLessThan(100);
    await page.close();
  });

  it("reaches the last row by scrolling", async () => {
    const page = await open("dark");
    await page.getByTestId("virtual-list").evaluate((el) => el.scrollTo({ top: el.scrollHeight }));
    await page.getByText("Row number 1999 with").waitFor();
    await page.close();
  });
});
