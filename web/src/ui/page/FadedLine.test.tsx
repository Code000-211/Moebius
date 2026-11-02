// @vitest-environment jsdom
/**
 * A line that stays on one line: the fade and the tooltip are the same answer to the same question — the words are
 * wider than the room — so they appear together, and neither appears when the line shows all of itself. The room is
 * what a test has to decide, since a headless browser measures nothing.
 */
import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { FadedLine } from "./FadedLine";

afterEach(cleanup);

const WHOLE = 'grep -nE "forge|github" src';

/** Draws the line in a box `room` wide whose words measure `words`, and hands back the box. */
function draw(room: number, words: number, keep?: "head" | "tail") {
  const rect = Element.prototype.getBoundingClientRect;
  const client = Object.getOwnPropertyDescriptor(Element.prototype, "clientWidth");
  Element.prototype.getBoundingClientRect = () => ({ width: words }) as DOMRect;
  Object.defineProperty(Element.prototype, "clientWidth", { configurable: true, value: room });
  try {
    const { container } = render(
      <FadedLine text={WHOLE} title={WHOLE} keep={keep} className="flex-1" />,
    );
    return container.firstElementChild as HTMLElement;
  } finally {
    Element.prototype.getBoundingClientRect = rect;
    if (client) Object.defineProperty(Element.prototype, "clientWidth", client);
    else delete (Element.prototype as unknown as Record<string, unknown>).clientWidth;
  }
}

describe("a line that stays on one line", () => {
  it("holds the whole of it where it can be read once it runs out", () => {
    const box = draw(100, 300);
    expect(box.getAttribute("title")).toBe(WHOLE);
    // A command keeps its end, which is the argument rather than the program.
    expect(box.className).toContain("fade-tail");
  });

  it("keeps the beginning when the name is what identifies the row", () => {
    const box = draw(100, 300, "head");
    expect(box.getAttribute("title")).toBe(WHOLE);
    expect(box.className).toContain("fade-end");
  });

  it("says nothing about a line that shows all of itself", () => {
    const box = draw(300, 100);
    expect(box.hasAttribute("title")).toBe(false);
    expect(box.className).not.toContain("fade-");
  });
});
