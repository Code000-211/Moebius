/**
 * The detector of coloured state pills, run inside a page by `page.evaluate(findPills)`. A state is a status icon and a
 * word, never a small box tinted with the colour of its state. The design audit (`audit.test.ts`) runs it on every
 * page, and `pills.e2e.ts` runs it in CI on the pages that once carried pills and on a few made-up elements that it
 * must and must not find.
 *
 * An element is a pill when it holds a short word or two, is as short as a control (28px) or fully rounded, and
 * either is filled with the tint of one of the four states (progress, attention, complete, error) or draws a line in
 * the colour of one. The colours are read from the theme and compared after being laid over what is behind them, so a
 * pill is found whatever class, opacity or radius built it. A banner, which is taller and holds a sentence, a
 * neutral chip, and a button or link (an action, such as a red Delete) are left alone.
 *
 * The function has to stand alone: Playwright sends its source to the page, so it may not use anything imported.
 */
export function findPills(): string[] {
  const resolver = document.createElement("canvas").getContext("2d", { willReadFrequently: true });
  type Rgba = [number, number, number, number];
  /** Any CSS colour, `color-mix` and `oklab` included, as red, green, blue and alpha. */
  const resolve = (css: string): Rgba => {
    if (!resolver) return [0, 0, 0, 0];
    resolver.clearRect(0, 0, 1, 1);
    resolver.fillStyle = "#000";
    resolver.fillStyle = css;
    resolver.fillRect(0, 0, 1, 1);
    const [r = 0, g = 0, b = 0, a = 0] = resolver.getImageData(0, 0, 1, 1).data;
    return [r, g, b, a / 255];
  };
  const over = (front: Rgba, back: Rgba): Rgba => [
    front[0] * front[3] + back[0] * (1 - front[3]),
    front[1] * front[3] + back[1] * (1 - front[3]),
    front[2] * front[3] + back[2] * (1 - front[3]),
    1,
  ];
  const near = (one: Rgba, two: Rgba) =>
    Math.max(Math.abs(one[0] - two[0]), Math.abs(one[1] - two[1]), Math.abs(one[2] - two[2])) <= 6;
  const root = getComputedStyle(document.documentElement);
  const canvas = resolve(getComputedStyle(document.body).backgroundColor);
  const behind = (el: Element): Rgba => {
    const stack: Rgba[] = [];
    for (let node = el.parentElement; node; node = node.parentElement) {
      const colour = resolve(getComputedStyle(node).backgroundColor);
      if (colour[3] > 0) stack.push(colour);
    }
    let seen: Rgba = canvas[3] > 0 ? canvas : [255, 255, 255, 1];
    for (let at = stack.length - 1; at >= 0; at -= 1) seen = over(stack[at] as Rgba, seen);
    return seen;
  };
  const states = ["progress", "attention", "complete", "error"].map((state) => ({
    state,
    fill: resolve(root.getPropertyValue(`--${state}-fill`).trim()),
    text: resolve(root.getPropertyValue(`--${state}-text`).trim()),
  }));

  const found: string[] = [];
  for (const el of Array.from(document.querySelectorAll("body *"))) {
    const text = (el.textContent ?? "").replace(/\s+/g, " ").trim();
    if (text.length === 0 || text.length > 40) continue;
    if (el.closest(".xterm, .cm-editor, [data-audit='skip']")) continue;
    // A button or a link is an action, and an action may carry a colour (the red of a delete); it is not a state.
    if (el.closest("button, a, [role='button'], [role='menuitem']")) continue;
    const style = getComputedStyle(el);
    if (style.visibility === "hidden" || style.display === "none") continue;
    if (Number(style.opacity) < 0.05) continue;
    const box = el.getBoundingClientRect();
    if (box.width <= 1 || box.height <= 1) continue;
    const round = Math.max(
      ...style.borderRadius.split(/[\s/]+/).map((value) => Number.parseFloat(value) || 0),
    );
    if (!(round > 0 && (box.height <= 28 || round >= box.height / 2))) continue;

    const fill = resolve(style.backgroundColor);
    const back = behind(el);
    const below = fill[3] > 0 ? over(fill, back) : null;
    const filled = below ? states.find((each) => near(below, over(each.fill, back))) : undefined;
    const line = states.find((each) =>
      (["Top", "Right", "Bottom", "Left"] as const).some((side) => {
        const width = Number.parseFloat(style[`border${side}Width`]) || 0;
        const colour = resolve(style[`border${side}Color`]);
        return (
          width > 0 &&
          style[`border${side}Style`] !== "none" &&
          colour[3] > 0.3 &&
          near(colour, each.text)
        );
      }),
    );
    const state = filled ?? line;
    if (!state) continue;
    const node = el as HTMLElement;
    const classes = Array.from(node.classList).slice(0, 3).join(".");
    found.push(
      `${node.tagName.toLowerCase()}${classes ? `.${classes}` : ""} a ${state.state} pill stands for a state; use a status icon and a word "${text.slice(0, 30)}"`,
    );
  }
  return found;
}
