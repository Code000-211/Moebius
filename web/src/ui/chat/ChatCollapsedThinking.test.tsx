// @vitest-environment jsdom
/**
 * The row of folded thinking says it opens. The caret that says so is drawn whatever the pointer is doing — a person at
 * the keyboard sees the same affordance — and it turns from the state the row is in.
 */
import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { ChatCollapsedThinking, thinkingLabel } from "./ChatCollapsedThinking";

afterEach(cleanup);

const entry = { content: "Reading the composer first.", expansionKey: "thinking-1" };

function drawn(expanded: boolean) {
  return (
    <ChatCollapsedThinking
      entries={[entry]}
      expanded={expanded}
      onToggle={() => {}}
      renderMarkdown={() => null}
    />
  );
}

describe("the row of folded thinking", () => {
  it("draws its caret with no pointer over it", () => {
    const { container } = render(drawn(false));
    const row = container.querySelector("button");
    expect(row?.getAttribute("aria-expanded")).toBe("false");
    expect(row?.querySelector("svg")).not.toBeNull();
  });

  it("turns the caret with the state, not with the pointer", () => {
    const { container, rerender } = render(drawn(false));
    const caret = () => container.querySelector("button svg")?.getAttribute("class") ?? "";
    expect(caret()).toContain("transition-transform");
    expect(caret()).not.toContain("rotate-90");
    rerender(drawn(true));
    expect(container.querySelector("button")?.getAttribute("aria-expanded")).toBe("true");
    expect(caret()).toContain("rotate-90");
  });
});

/**
 * The row says what happened and, once it has stopped, for how long. A thought still arriving is said in the present
 * tense and without a length, because the length is the one thing nobody knows yet — and a thought that ended with
 * nothing after it has no length either, so it keeps the bare word rather than "Thought 0s".
 */
describe("the word for a thought", () => {
  it("says it in the present tense while it is arriving, whatever the clock says", () => {
    expect(thinkingLabel(null, true)).toBe("Thinking");
    expect(thinkingLabel(9, true)).toBe("Thinking");
  });

  it("says it in the past, without a length it never had", () => {
    expect(thinkingLabel(null, false)).toBe("Thought");
    expect(thinkingLabel(undefined, false)).toBe("Thought");
    expect(thinkingLabel(0, false)).toBe("Thought");
  });

  it("adds how long it took once it has stopped", () => {
    expect(thinkingLabel(12, false)).toBe("Thought 12s");
    expect(thinkingLabel(95, false)).toBe("Thought 1m");
  });

  it("is the words the folded row says out loud", () => {
    const { container } = render(
      <ChatCollapsedThinking
        entries={[entry]}
        expanded={false}
        onToggle={() => {}}
        renderMarkdown={() => null}
        seconds={12}
      />,
    );
    expect(container.querySelector("button")?.textContent).toBe("Thought 12s");
  });
});
