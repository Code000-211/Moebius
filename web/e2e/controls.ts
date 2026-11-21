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

/** Set a filter of the shared bar by mouse: "+ Filter", the field, then its value, as the menus show them. */
export async function addFilter(
  page: Page,
  bar: Locator,
  field: string,
  value: string,
): Promise<void> {
  await bar.getByRole("button", { name: "Filter", exact: true }).click();
  await page.getByRole("menuitem", { name: field, exact: true }).click();
  await page.getByRole("menuitemradio", { name: value, exact: true }).click();
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
 * Checks how a page which puts a list beside a detail holds its two columns. The window is made short first, so the
 * detail really has more behind its edge than fits, and then: neither the page nor the document scrolls — a column
 * that leaks past the window, as an absolutely placed thing inside a scroller can, gives the whole UI a second
 * scrollbar; the detail scrolls where it stands and leaves the list where it was; and each column ends at the bottom
 * edge of the pane that scrolls the page rather than a band of padding above it.
 */
export async function columnsScrollEach(page: Page): Promise<void> {
  await page.setViewportSize({ width: 1280, height: 320 });
  const frame = page.locator("main");
  const list = page.getByTestId("list-column");
  const detail = page.getByTestId("detail-column");
  const overflow = (locator: Locator) =>
    locator.evaluate((el) => el.scrollHeight - el.clientHeight);
  const top = (locator: Locator) => locator.evaluate((el) => el.scrollTop);
  const edges = () =>
    page.evaluate(() => {
      const doc = document.scrollingElement;
      // A column takes back 16px of the bottom padding of the pane that scrolls the page (`-mb-4`); a pane that keeps
      // more (the settings pages keep 24) leaves that much more between the column and its edge, and no more.
      const read = (testId: string) => {
        const el = document.querySelector(`[data-testid="${testId}"]`);
        if (!el) return null;
        let pane = el.parentElement;
        while (pane && !/^(auto|scroll)$/.test(getComputedStyle(pane).overflowY))
          pane = pane.parentElement;
        const bottom = pane ? pane.getBoundingClientRect().bottom : window.innerHeight;
        const kept = pane ? Number.parseFloat(getComputedStyle(pane).paddingBottom) : 0;
        return Math.round(bottom - el.getBoundingClientRect().bottom - Math.max(0, kept - 16));
      };
      return {
        document: doc ? doc.scrollHeight - doc.clientHeight : 0,
        list: read("list-column"),
        detail: read("detail-column"),
      };
    });
  await expect
    .poll(() => overflow(frame), { message: "the page scrolls as one sheet" })
    .toBeLessThan(8);
  await expect
    .poll(edges, { message: "something inside the columns reaches past the window" })
    .toHaveProperty("document", 0);
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
  // Each column runs to the bottom edge of the page's pane: no band of padding under it.
  const reached = await edges();
  expect(Math.abs(reached.list ?? 0), "the list stops short of the page's edge").toBeLessThan(2);
  expect(Math.abs(reached.detail ?? 0), "the detail stops short of the page's edge").toBeLessThan(
    2,
  );
}

/**
 * Checks how an open editor window holds in a short browser window: the window lies inside the window of the browser,
 * the document does not scroll, and the window's body scrolls between its head and foot, which stay where they are.
 */
export async function editorWindowFits(page: Page): Promise<void> {
  await page.setViewportSize({ width: 1280, height: 320 });
  const win = page.getByTestId("editor-window");
  await win.waitFor();
  const body = win
    .locator("> div")
    .filter({ has: page.locator("input, textarea, [role=combobox]") });
  const scroller = body.first();
  await expect
    .poll(() => scroller.evaluate((el) => el.scrollHeight - el.clientHeight), {
      message: "the body of the window has nothing to scroll",
    })
    .toBeGreaterThan(0);
  const placed = async () => {
    const box = await win.boundingBox();
    if (!box) throw new Error("the editor window drew no box");
    return box;
  };
  const box = await placed();
  const view = page.viewportSize() ?? { width: 1280, height: 320 };
  expect(box.y, "the window starts above the browser window").toBeGreaterThanOrEqual(0);
  expect(box.y + box.height, "the window reaches past the browser window").toBeLessThanOrEqual(
    view.height,
  );
  const reach = () =>
    page.evaluate(() => {
      const doc = document.scrollingElement;
      return doc ? doc.scrollHeight - doc.clientHeight : 0;
    });
  expect(await reach(), "the document scrolls").toBe(0);
  const inner = await scroller.boundingBox();
  if (!inner) throw new Error("the body of the window drew no box");
  await page.mouse.move(inner.x + inner.width / 2, inner.y + Math.min(inner.height / 2, 60));
  await page.mouse.wheel(0, 400);
  await expect
    .poll(() => scroller.evaluate((el) => el.scrollTop), { message: "the body did not scroll" })
    .toBeGreaterThan(0);
  expect(await placed(), "the window moved while its body scrolled").toEqual(box);
  expect(await reach(), "the document scrolls").toBe(0);
}

/** A point for the pointer to wait at that is outside `box`, so nothing of that pane is hovered. */
function parkOutside(
  box: { x: number; y: number; width: number; height: number },
  view: { width: number; height: number },
): { x: number; y: number } {
  const inPane = Math.round(box.y + Math.min(box.height / 2, 100));
  const left = Math.round(box.x - 8);
  if (left >= 1) return { x: left, y: inPane };
  const right = Math.round(box.x + box.width + 8);
  if (right <= view.width - 2) return { x: right, y: inPane };
  const above = Math.round(box.y - 8);
  if (above >= 1) return { x: Math.round(box.x + Math.min(box.width / 2, 100)), y: above };
  return {
    x: Math.round(box.x + Math.min(box.width / 2, 100)),
    y: Math.round(Math.min(box.y + box.height + 8, view.height - 2)),
  };
}

/**
 * Checks the app's rule for scrollbars over every pane of the page that runs off: a thumb is painted only while the
 * pointer is inside the pane it scrolls, and the pane keeps the same room — and its content the same place — whether
 * the thumb is painted or not, so a bar arriving or leaving moves nothing. Every pane that has more behind its bottom
 * edge than fits is read twice: once with the pointer parked outside it and once with the pointer in it. What has to
 * differ between the readings is the eight-wide strip along the pane's right edge, where its own bar is drawn and no
 * row of content reaches; what has not to differ is the width the pane has to lay its content in and where the pane
 * and its first row stand. `within` is a selector to look inside, for a window over the page: the panes of the page
 * behind its scrim are out of the pointer's reach.
 */
export async function scrollbarsAreQuiet(page: Page, within = "body"): Promise<void> {
  const marks = await page.evaluate((scope) => {
    const found: string[] = [];
    for (const el of Array.from(document.querySelectorAll<HTMLElement>(`${scope} *`))) {
      const style = getComputedStyle(el);
      const scrolls = style.overflowY === "auto" || style.overflowY === "scroll";
      if (!scrolls || el.scrollHeight - el.clientHeight < 24) continue;
      el.dataset.scrollbarPane = String(found.length);
      found.push(el.dataset.scrollbarPane);
    }
    return found;
  }, within);
  expect(marks.length, "the page has no pane that runs off to scroll").toBeGreaterThan(0);
  const view = page.viewportSize() ?? { width: 1280, height: 720 };

  for (const mark of marks) {
    const pane = page.locator(`[data-scrollbar-pane="${mark}"]`);
    const box = await pane.boundingBox();
    if (!box) throw new Error(`pane ${mark} drew no box`);
    // The strip is what the bar is painted on: inside the border box, where no row of content is drawn.
    const top = Math.max(0, Math.ceil(box.y));
    const clip = {
      x: Math.round(box.x + box.width) - 8,
      y: top,
      width: 8,
      height: Math.max(1, Math.round(Math.min(box.height, 160, view.height - top - 1))),
    };
    const reading = async () => ({
      strip: await page.screenshot({ clip }),
      state: await pane.evaluate((node0) => {
        const el = node0 as HTMLElement;
        const place = (node: Element) => {
          const r = node.getBoundingClientRect();
          return `${Math.round(r.x)},${Math.round(r.y)},${Math.round(r.width)},${Math.round(r.height)}`;
        };
        return {
          thumb: getComputedStyle(el).scrollbarColor,
          // What a border takes is not a bar: a pane with one (the settings navigation) would else look as if it had one.
          bar:
            el.offsetWidth -
            el.clientWidth -
            el.clientLeft -
            Number.parseFloat(getComputedStyle(el).borderRightWidth),
          room: `${el.clientWidth}x${el.clientHeight}`,
          pane: place(el),
          row: el.firstElementChild ? place(el.firstElementChild) : "",
        };
      }),
    });

    const park = parkOutside(box, view);
    await page.mouse.move(park.x, park.y);
    const quiet = await reading();
    expect(quiet.state.thumb, `pane ${mark} paints a thumb while nothing of it is hovered`).toMatch(
      /^rgba\(0, 0, 0, 0\)/,
    );

    await page.mouse.move(box.x + box.width / 2, box.y + Math.min(box.height / 2, 100));
    await expect
      .poll(() => pane.evaluate((el) => getComputedStyle(el).scrollbarColor), {
        message: `pane ${mark} keeps its thumb hidden with the pointer inside it`,
      })
      .not.toMatch(/^rgba\(0, 0, 0, 0\)/);
    const shown = await reading();

    // A bar that takes room in the pane is painted from the colours of the page, so the strip says whether the thumb
    // arrived. A bar that floats over the content instead — the flavour a headless browser draws, where a pane's
    // offset width and client width are the same — is painted by the browser itself and answers to no colour of the
    // page, so there is nothing in the strip to read and only the room is worth checking.
    if (shown.state.bar > 0) {
      expect(
        Buffer.compare(quiet.strip, shown.strip),
        `pane ${mark} shows no thumb of its own when the pointer is inside it`,
      ).not.toBe(0);
      await page.mouse.move(park.x, park.y);
      // And it goes when the hand goes: the bar belongs to the pointer being inside, not to having been inside.
      const away = await reading();
      expect(
        Buffer.compare(quiet.strip, away.strip),
        `pane ${mark} keeps its thumb out after the pointer leaves it`,
      ).toBe(0);
    }
    expect(shown.state.bar, `pane ${mark} gains or loses room for its bar on hover`).toBe(
      quiet.state.bar,
    );
    expect(shown.state.room, `pane ${mark} takes a different room once its thumb is painted`).toBe(
      quiet.state.room,
    );
    expect(shown.state.pane, `pane ${mark} moves when its thumb is painted`).toBe(quiet.state.pane);
    expect(shown.state.row, `pane ${mark} shifts its content when its thumb is painted`).toBe(
      quiet.state.row,
    );
  }
}

/** Opens the fold "More properties" under a card window's compact block of properties, if it is shut. */
export async function unfold(properties: Locator): Promise<void> {
  const fold = properties.locator("details");
  if (!(await fold.evaluate((el) => (el as HTMLDetailsElement).open))) {
    await fold.locator("summary").click();
  }
}
