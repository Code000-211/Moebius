// @vitest-environment jsdom
/**
 * A card cuts a description short and says so: the last line it shows fades out only when there really are lines
 * behind it, so a short description is drawn plain rather than ending in a fade that promises more. A headless
 * browser lays nothing out, so the test is the one that decides how tall the box and its content are.
 */
import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { FadedClamp } from "./FadedClamp";

afterEach(cleanup);

/** Draw `text` as though the box were `room` tall and its writing `content` tall. */
function draw({ content, room, text }: { content: number; room: number; text?: string }) {
  const held = ["scrollHeight", "clientHeight"].map(
    (name) => [name, Object.getOwnPropertyDescriptor(Element.prototype, name)] as const,
  );
  for (const [name, amount] of [
    ["scrollHeight", content],
    ["clientHeight", room],
  ] as const) {
    Object.defineProperty(Element.prototype, name, { configurable: true, value: amount });
  }
  try {
    const { container } = render(<FadedClamp text={text ?? "A description of some length"} />);
    return container.firstElementChild as HTMLElement;
  } finally {
    for (const [name, descriptor] of held) {
      if (descriptor) Object.defineProperty(Element.prototype, name, descriptor);
      else delete (Element.prototype as unknown as Record<string, unknown>)[name];
    }
  }
}

describe("a card that cuts a description short", () => {
  it("always stops at three lines", () => {
    expect(draw({ content: 60, room: 60 }).className).toContain("clamp-3");
    expect(draw({ content: 600, room: 60 }).className).toContain("clamp-3");
  });

  it("fades the last line when the writing runs past the box", () => {
    expect(draw({ content: 600, room: 60 }).className).toContain("fade-last-3");
  });

  it("leaves a description that fits the three lines alone", () => {
    expect(draw({ content: 60, room: 60 }).className).not.toContain("fade-last-3");
  });

  it("says the writing it holds", () => {
    expect(draw({ content: 60, room: 60, text: "the issue in short" }).textContent).toBe(
      "the issue in short",
    );
  });
});
