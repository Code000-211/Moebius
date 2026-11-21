/**
 * Small assertion functions for the quiet rules and the scrolling rules of `docs/design-web.md` (Motion, Scrolling
 * performance). Each takes a page of a real browser, looks only at what is drawn (computed styles, animations, a count
 * of elements, a measured width) and throws an `Error` that names what broke the rule; none of them measures time.
 * They are written against the standard, not against the markup of a page, so a page may change its classes and test
 * ids without touching them. `perf.e2e.ts` runs them on the pages and, with a fixture that breaks each rule, checks
 * that each one really throws.
 */
import type { Page } from "playwright";

/** What a guard says when the rule holds nowhere: an empty list passes, anything else throws. */
function expectNone(rule: string, found: string[]): void {
  if (found.length === 0) return;
  const shown = found.slice(0, 8).join("; ");
  const more = found.length > 8 ? `; and ${found.length - 8} more` : "";
  throw new Error(`${rule}: ${found.length} found: ${shown}${more}`);
}

/** `div.class[data-testid]` of an element, short enough to read in a failure. */
const NAME = `(el) => {
  const id = el.getAttribute("data-testid");
  const cls = typeof el.className === "string" ? el.className.trim().split(/\\s+/).slice(0, 3).join(".") : "";
  return el.tagName.toLowerCase() + (cls ? "." + cls : "") + (id ? "[" + id + "]" : "");
}`;

/** No element, and no pseudo-element of one, has a mask image: a scroller with an edge fade is drawn through a mask. */
export async function noMaskImage(page: Page): Promise<void> {
  const found = await page.evaluate(`(() => {
    const name = ${NAME};
    const found = [];
    for (const el of document.querySelectorAll("*")) {
      for (const pseudo of [null, "::before", "::after"]) {
        const style = getComputedStyle(el, pseudo);
        const mask = style.maskImage || style.webkitMaskImage || "none";
        if (mask !== "none") found.push(name(el) + (pseudo ?? "") + " " + mask.slice(0, 40));
      }
    }
    return found;
  })()`);
  expectNone("an element has a mask-image", found as string[]);
}

/** What may blur what is behind it: a menu, a tooltip, the palette (all three are `.glass`). */
export const MAY_BLUR = ".glass, [role='menu'], [role='tooltip']";
/** The largest blur, in px, the standard allows. */
export const MAX_BLUR = 16;

/**
 * Nothing has a backdrop filter except what `allowed` matches, and that one blurs by no more than 16px. The root is
 * told `data-blur="on"` first, since headless Chromium reports a software renderer and the app then keeps blur off by
 * itself.
 */
export async function noBackdropBlur(
  page: Page,
  allowed: string = MAY_BLUR,
  max: number = MAX_BLUR,
): Promise<void> {
  const found = await page.evaluate(
    ({ allowed, max }) => {
      document.documentElement.dataset.blur = "on";
      const found: string[] = [];
      for (const el of Array.from(document.querySelectorAll("*"))) {
        const style = getComputedStyle(el);
        const filter = style.backdropFilter || "none";
        if (filter === "none") continue;
        const name = `${el.tagName.toLowerCase()}.${String(el.className).trim().split(/\s+/).slice(0, 3).join(".")}`;
        if (!el.matches(allowed)) {
          found.push(`${name} blurs: ${filter}`);
          continue;
        }
        for (const match of filter.matchAll(/blur\(\s*([\d.]+)px/g)) {
          if (Number(match[1]) > max) found.push(`${name} blurs by ${match[1]}px, over ${max}px`);
        }
      }
      return found;
    },
    { allowed, max },
  );
  expectNone("a surface has a backdrop-filter", found);
}

/** The loading spinner and the pulse of a pending switch are the infinite animations the app has. */
export const MAY_REPEAT = ["spin", "pulse"];

/**
 * No animation runs forever, except the named ones, and those only on an element that is in the window: nobody
 * is looking at one that is not. Both the animations the document is running and the ones the style asks for are read.
 */
export async function noInfiniteAnimation(
  page: Page,
  allowed: string[] = MAY_REPEAT,
): Promise<void> {
  const found = await page.evaluate((allowed) => {
    const found: string[] = [];
    const inView = (el: Element) => {
      const box = el.getBoundingClientRect();
      return (
        box.width > 0 &&
        box.height > 0 &&
        box.bottom > 0 &&
        box.right > 0 &&
        box.top < window.innerHeight &&
        box.left < window.innerWidth
      );
    };
    const say = (el: Element | null, animation: string) =>
      `${el ? el.tagName.toLowerCase() : "?"} runs "${animation}" forever${el && !inView(el) ? " off screen" : ""}`;
    for (const animation of document.getAnimations()) {
      if (animation.effect?.getComputedTiming().iterations !== Number.POSITIVE_INFINITY) continue;
      const target = (animation.effect as KeyframeEffect).target;
      const name = (animation as CSSAnimation).animationName ?? "(script)";
      if (!allowed.includes(name) || !target || !inView(target)) found.push(say(target, name));
    }
    for (const el of Array.from(document.querySelectorAll("*"))) {
      const style = getComputedStyle(el);
      const names = style.animationName.split(",").map((each) => each.trim());
      const counts = style.animationIterationCount.split(",").map((each) => each.trim());
      names.forEach((name, at) => {
        if (name === "none" || counts[at % counts.length] !== "infinite") return;
        if (!allowed.includes(name) || !inView(el)) found.push(say(el, name));
      });
    }
    return Array.from(new Set(found));
  }, allowed);
  expectNone("an animation never ends", found);
}

/** What a transition may not move: the size or the place of a box makes the page around it reflow. */
const LAYOUT_PROPERTIES = /^(all|width|height|margin(-.*)?|padding(-.*)?|top|left)$/;

/** No element, nor pseudo-element, has a transition of some length on its size, its spacing or its place. */
export async function noLayoutTransition(page: Page): Promise<void> {
  const found = await page.evaluate((source) => {
    const banned = new RegExp(source);
    const found: string[] = [];
    for (const el of Array.from(document.querySelectorAll("*"))) {
      for (const pseudo of [null, "::before", "::after"]) {
        const style = getComputedStyle(el, pseudo);
        const properties = style.transitionProperty.split(",").map((each) => each.trim());
        const lengths = style.transitionDuration.split(",").map((each) => Number.parseFloat(each));
        properties.forEach((property, at) => {
          const length = lengths[at % lengths.length] ?? 0;
          if (length > 0 && banned.test(property)) {
            found.push(`${el.tagName.toLowerCase()}${pseudo ?? ""} transitions ${property}`);
          }
        });
      }
    }
    return Array.from(new Set(found));
  }, LAYOUT_PROPERTIES.source);
  expectNone("a transition moves the layout", found);
}

/** Waits until every transition and animation that has an end has ended, so a style is read where it settled. */
async function settled(page: Page): Promise<void> {
  await page.evaluate(() =>
    Promise.all(
      document
        .getAnimations()
        .filter((a) => a.effect?.getComputedTiming().iterations !== Number.POSITIVE_INFINITY)
        .map((a) => a.finished.catch(() => {})),
    ),
  );
}

/** The look of an element that a lift would change: its transform, its shadow and where it stands. */
const looks = (el: Element) => {
  const style = getComputedStyle(el);
  const box = el.getBoundingClientRect();
  return `${style.transform} | ${style.boxShadow} | ${box.x},${box.y},${box.width},${box.height}`;
};

/** The thing under the pointer does not lift, grow or gain a shadow: its look is the same once the hover has settled. */
export async function noHoverLift(page: Page, selector: string): Promise<void> {
  const target = page.locator(selector).first();
  await page.mouse.move(2, 2);
  await settled(page);
  const before = await target.evaluate(looks);
  await target.hover();
  await settled(page);
  const after = await target.evaluate(looks);
  if (before !== after) {
    throw new Error(`the pointer changes ${selector}: ${before} became ${after}`);
  }
}

/** The page holds fewer than `max` of what `selector` finds: a long list draws only the rows in view. */
export async function fewRows(page: Page, selector: string, max = 100): Promise<number> {
  const drawn = await page.locator(selector).count();
  if (drawn === 0) throw new Error(`no ${selector} is drawn, so the count says nothing`);
  if (drawn >= max) throw new Error(`${drawn} of ${selector} are drawn, ${max} is the most`);
  return drawn;
}

/** The width and place of the first and the last of `selector`, joined: what a row of columns takes of the page. */
const span = (selector: string) => `(() => {
  const boxes = Array.from(document.querySelectorAll(${JSON.stringify(selector)})).map((el) => el.getBoundingClientRect());
  if (boxes.length === 0) return null;
  const left = Math.min(...boxes.map((box) => box.left));
  const right = Math.max(...boxes.map((box) => box.right));
  return left + ".." + right;
})()`;

/** Opening something (`open`) does not move the columns of `selector`: the row of them stands as wide, to the pixel. */
export async function widthUnchanged(
  page: Page,
  selector: string,
  open: () => Promise<void>,
): Promise<void> {
  const before = await page.evaluate(span(selector));
  if (before === null) throw new Error(`nothing matches ${selector}`);
  await open();
  await settled(page);
  const after = await page.evaluate(span(selector));
  if (after !== before) throw new Error(`${selector} was ${before} and is ${after}`);
}

/** Every quiet rule that holds on a page as it stands. */
export async function quiet(page: Page): Promise<void> {
  await noMaskImage(page);
  await noBackdropBlur(page);
  await noInfiniteAnimation(page);
  await noLayoutTransition(page);
}
