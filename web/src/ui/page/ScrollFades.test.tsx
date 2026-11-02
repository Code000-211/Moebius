// @vitest-environment jsdom
/**
 * A scroller fades the edges that have more behind them and leaves the others alone: a fade over content that is all
 * there is a lie, and a missing fade over content that is cut off reads as the text ending mid-word. Where the view
 * sits is what a test has to decide, since a headless browser measures nothing.
 */
import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { ScrollFades } from "./ScrollFades";

afterEach(cleanup);

/** The mask of a scroller whose content is `wide` x `tall` in a box `roomW` x `roomH`, looking at `left`, `top`. */
function draw(over: {
  wide?: number;
  tall?: number;
  roomW?: number;
  roomH?: number;
  left?: number;
  top?: number;
}) {
  const held = [
    "scrollWidth",
    "clientWidth",
    "scrollLeft",
    "scrollHeight",
    "clientHeight",
    "scrollTop",
  ].map((name) => [name, Object.getOwnPropertyDescriptor(Element.prototype, name)] as const);
  const value = {
    scrollWidth: over.wide ?? 200,
    clientWidth: over.roomW ?? 200,
    scrollLeft: over.left ?? 0,
    scrollHeight: over.tall ?? 200,
    clientHeight: over.roomH ?? 200,
    scrollTop: over.top ?? 0,
  } as Record<string, number>;
  for (const [name, amount] of Object.entries(value)) {
    Object.defineProperty(Element.prototype, name, { configurable: true, value: amount });
  }
  try {
    const { container } = render(
      <ScrollFades>
        <div />
      </ScrollFades>,
    );
    return (container.firstElementChild as HTMLElement).getAttribute("style") ?? "";
  } finally {
    for (const [name, descriptor] of held) {
      if (descriptor) Object.defineProperty(Element.prototype, name, descriptor);
      else delete (Element.prototype as unknown as Record<string, unknown>)[name];
    }
  }
}

/** A layer that hides nothing, for the axis that has nothing behind either end of it. */
const SOLID = "linear-gradient(currentColor, currentColor)";

describe("a scroller that says what is behind its edges", () => {
  it("fades nothing when the box shows all of the content", () => {
    const style = draw({});
    expect(style).toContain(SOLID);
    expect(style).not.toContain("transparent");
  });

  it("fades the bottom of a box that has more below it", () => {
    const style = draw({ tall: 600, roomH: 200, top: 0 });
    expect(style).toContain("to top, transparent");
    expect(style).not.toContain("to bottom, transparent");
  });

  it("fades the top of a box that has been scrolled down", () => {
    const style = draw({ tall: 600, roomH: 200, top: 400 });
    expect(style).toContain("to bottom, transparent");
    expect(style).not.toContain("to top, transparent");
  });

  it("fades both ends of a box that sits in the middle", () => {
    // One layer does both ends of an axis: the ramp in at the top, the same ramp in at the bottom.
    const style = draw({ tall: 600, roomH: 200, top: 200 });
    expect(style).toContain("to bottom, transparent");
    expect(style).toContain("calc(100% - var(--space-24)), transparent");
  });

  it("fades the left of a box whose lane runs off to the side", () => {
    const style = draw({ wide: 900, roomW: 300, left: 100 });
    expect(style).toContain("to right, transparent");
    expect(style).not.toContain("to left, transparent");
  });
});
