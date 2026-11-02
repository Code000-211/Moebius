/**
 * How a test uses the app's own controls. There is no native `<select>` in the app: a choice is a button that opens
 * the app's menu (`Select` in `web/src/ui/page/Field.tsx`, `PillSelect` in `web/src/ui/chat/PillSelect.tsx`), so it is
 * opened and answered the way a person answers it. A menu is drawn at the end of the document rather than inside the
 * field that opened it, which is why every one of these takes the page as well as the trigger. The last measure is of
 * the page rather than a control: a list beside a detail is checked to hold both inside the window.
 */
import type { Locator, Page } from "playwright";
import { expect } from "vitest";

/** Open `trigger` and choose the row that shows `option`. */
export async function pickOption(page: Page, trigger: Locator, option: string): Promise<void> {
  await trigger.click();
  await page.getByRole("menuitemradio", { name: option, exact: true }).click();
}

/** The words a choice shows on its own trigger, which are what stands now. */
export async function showing(trigger: Locator): Promise<string> {
  const text = await trigger.textContent();
  return (text ?? "").trim();
}

/** The words of every row a choice would offer, in the order it shows them, then closes. */
export async function offers(page: Page, trigger: Locator): Promise<string[]> {
  await trigger.click();
  const rows = await page.getByRole("menuitemradio").allTextContents();
  await page.keyboard.press("Escape");
  return rows.map((each) => each.trim());
}

/**
 * Checks that a page which puts a list beside a detail holds both inside the window and scrolls each of them on its
 * own. The window is made short first, so the detail really has more behind its edge than fits: the page itself does
 * not scroll, the detail scrolls where it stands, and the list is left where it was. A page that scrolls as one sheet
 * fails the first measure, and one whose columns are not bounded by the window fails it too.
 */
export async function columnsScrollEach(page: Page): Promise<void> {
  await page.setViewportSize({ width: 1280, height: 520 });
  const frame = page.locator("main");
  const list = page.getByTestId("list-column");
  const detail = page.getByTestId("detail-column");
  const overflow = (locator: Locator) =>
    locator.evaluate((el) => el.scrollHeight - el.clientHeight);
  const top = (locator: Locator) => locator.evaluate((el) => el.scrollTop);
  await expect
    .poll(() => overflow(frame), { message: "the page scrolls as one sheet" })
    .toBeLessThan(8);
  await expect
    .poll(() => overflow(detail), { message: "the detail has nothing to scroll" })
    .toBeGreaterThan(0);
  const box = await detail.boundingBox();
  if (!box) throw new Error("the detail column drew no box");
  await page.mouse.move(box.x + box.width / 2, box.y + 120);
  await page.mouse.wheel(0, 400);
  await expect.poll(() => top(detail), { message: "the detail did not scroll" }).toBeGreaterThan(0);
  expect(await top(frame), "the page scrolled with the detail").toBe(0);
  expect(await top(list), "the list scrolled with the detail").toBe(0);
}
