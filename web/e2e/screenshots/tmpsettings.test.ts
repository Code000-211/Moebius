import { chromium } from "playwright";
import { expect, it } from "vitest";
import { startFakeServer } from "../fakeServer";
import { snapshot } from "./showcase";
const OUT = "/tmp/claude-0/-home-user-Moebius/232c1905-704d-50fe-bb3e-6f4bac343810/scratchpad/shots";
it("shoots", async () => {
  const browser = await chromium.launch({ executablePath: process.env.MOEBIUS_E2E_CHROMIUM || undefined });
  const fake = await startFakeServer();
  for (const theme of ["light", "dark"] as const) {
    const ctx = await browser.newContext({ viewport: { width: 1280, height: 720 }, deviceScaleFactor: 2, colorScheme: theme });
    const page = await ctx.newPage();
    const r = await page.request.post(`${fake.url}/v1/session`, { headers: { Origin: fake.url, "X-Moebius": "1" }, data: { token: fake.token } });
    expect(r.status()).toBe(204);
    await page.route("**/v1/snapshot", (route) => route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(snapshot()) }));
    for (const p of ["appearance", "profile", "repositories", "mcp-servers"]) {
      await page.goto(`${fake.url}/${p}`);
      await page.getByRole("navigation", { name: "Settings" }).waitFor();
      await page.waitForLoadState("networkidle").catch(() => {});
      await page.waitForTimeout(700);
      await page.screenshot({ path: `${OUT}/m2-${p}-${theme}.png` });
    }
    await ctx.close();
  }
  await browser.close();
  await fake.stop().catch(() => {});
}, 200000);
