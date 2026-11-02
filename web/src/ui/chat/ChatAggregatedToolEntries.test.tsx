// @vitest-environment jsdom
/**
 * A group of reads is one row that opens. The caret that says so is always drawn, so the affordance does not wait for a
 * pointer, and the count of what was read agrees with itself.
 */
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { ChatAggregatedToolEntries } from "./ChatAggregatedToolEntries";

afterEach(cleanup);

const reads = [
  { summary: "web/src/app/App.tsx", expansionKey: "read-1" },
  { summary: "web/src/app/board/model.ts", expansionKey: "read-2" },
];

function drawn(expanded: boolean) {
  return (
    <ChatAggregatedToolEntries
      entries={reads}
      expanded={expanded}
      onToggle={() => {}}
      label="Read"
      unit="file"
    />
  );
}

describe("a group of reads", () => {
  it("counts what it read", () => {
    render(drawn(false));
    expect(screen.getByText("2 files")).toBeTruthy();
  });

  it("draws its caret with no pointer over it, and turns it with the state", () => {
    const { container, rerender } = render(drawn(false));
    const row = container.querySelector("button");
    const caret = () => row?.querySelector("svg:last-of-type")?.getAttribute("class") ?? "";
    expect(row?.getAttribute("aria-expanded")).toBe("false");
    expect(caret()).toContain("-rotate-90");
    rerender(drawn(true));
    expect(caret()).not.toContain("-rotate-90");
  });
});
