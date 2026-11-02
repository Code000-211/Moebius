// @vitest-environment jsdom
/**
 * A changed file in the step list: the counts of lines agree with themselves when there is one of them, and the row is
 * as tall as the rows beside it, so the list does not step in and out as it is read down.
 */
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { ChatFileEntry } from "./ChatFileEntry";

afterEach(cleanup);

function draw(additions: number, deletions: number) {
  return render(
    <ChatFileEntry
      filename="web/src/ui/chat/Turn.tsx"
      additions={additions}
      deletions={deletions}
      expanded={false}
      onToggle={() => {}}
    />,
  );
}

describe("the counts of a changed file", () => {
  it("says one line as one line", () => {
    draw(1, 1);
    expect(screen.getByTitle("1 line added").textContent).toBe("+1");
    expect(screen.getByTitle("1 line removed").textContent).toBe("-1");
  });

  it("says several lines as several lines", () => {
    draw(12, 3);
    expect(screen.getByTitle("12 lines added").textContent).toBe("+12");
    expect(screen.getByTitle("3 lines removed").textContent).toBe("-3");
  });

  it("is a row of the step list, at the height the others are", () => {
    const { container } = draw(2, 1);
    const row = container.querySelector("button")?.getAttribute("class") ?? "";
    expect(row).toContain("min-h-8");
    expect(row).not.toContain("min-h-12");
  });
});
