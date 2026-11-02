// @vitest-environment jsdom
/**
 * Almost every tool call in a chat and in the Fleet finished, so a pill on each one hides the few rows that matter. A
 * call that finished is one quiet gray check that still says its word to a screen reader; anything unfinished is
 * still the word and its hue together, because the row has to be read.
 */
import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { statusWord, ToolStatusDot } from "./ToolStatusDot";

afterEach(cleanup);

/** The mark a status is drawn as: the element that carries it, and the classes it was given. */
function drawn(status: string, className?: string): HTMLElement {
  const { container } = render(<ToolStatusDot status={{ status }} className={className} />);
  const node = container.querySelector<HTMLElement>(`[data-status="${status}"]`);
  if (!node) throw new Error(`nothing drawn for ${status}`);
  return node;
}

describe("a tool's state", () => {
  it("is a quiet mark where the call finished", () => {
    const mark = drawn("success");
    expect(mark.querySelector("svg")).not.toBeNull();
    expect(mark.className).toContain("text-secondary");
    expect(mark.className).not.toContain("rounded-full");
    expect(mark.className).not.toContain("bg-complete");
  });

  it("keeps the word of a finished call for a screen reader", () => {
    expect(drawn("success").textContent).toBe(statusWord("success"));
  });

  it("is still the word and its hue where a call did not finish", () => {
    const pill = drawn("failed");
    expect(pill.textContent).toBe("Failed");
    expect(pill.className).toContain("rounded-full");
    expect(pill.className).toContain("bg-error-fill");
    expect(pill.querySelector("svg")).toBeNull();
  });

  it("carries the classes of the row it stands in, either way", () => {
    expect(drawn("success", "shrink-0").className).toContain("shrink-0");
    expect(drawn("pending_approval", "shrink-0").className).toContain("shrink-0");
  });
});
