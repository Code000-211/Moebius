/**
 * How a test uses the app's own controls. There is no native `<select>` in the app: a choice is a button that opens
 * the app's menu (`Select` in `web/src/ui/page/Field.tsx`, `PillSelect` in `web/src/ui/chat/PillSelect.tsx`), so it is
 * opened and answered the way a person answers it. A menu is drawn at the end of the document rather than inside the
 * field that opened it, which is why every one of these takes the page as well as the trigger.
 */
import type { Locator, Page } from "playwright";

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
