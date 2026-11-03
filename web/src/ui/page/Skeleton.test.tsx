// @vitest-environment jsdom
/**
 * A placeholder takes the shape of what is coming rather than drawing a spinner, so two things have to hold: the bars
 * say nothing — the only words in the region are the sentence that says the page is being read, and they are spoken
 * rather than drawn — and the region has the shape the page will fill, the number of rows or lanes it will have.
 */
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { Reading, SkeletonBoard, SkeletonCard, SkeletonList, SkeletonThread } from "./Skeleton";

afterEach(cleanup);

/** The bars of the rendered placeholder: everything in it that is hidden from a screen reader. */
function bars() {
  return Array.from(document.querySelectorAll<HTMLElement>("[aria-hidden='true']"));
}

describe("a placeholder for a page being read", () => {
  it("says what it is reading in words nobody has to look at", () => {
    render(<SkeletonList rows={3} message="Reading the skills" />);
    const region = screen.getByRole("status");
    expect(region.textContent).toBe("Reading the skills");
    // The message is the only text in the region; the rows are drawn, not written.
    for (const bar of bars()) expect(bar.textContent).toBe("");
  });

  it("draws the rows the list will have, and no more words than that", () => {
    render(<SkeletonList rows={3} message="Reading the skills" />);
    const region = screen.getByRole("status");
    expect(region.querySelectorAll(".rounded-control")).toHaveLength(3);
    expect(bars().length).toBeGreaterThan(3);
  });

  it("draws as many lanes as the board will have", () => {
    render(<SkeletonBoard columns={3} message="Reading the board" />);
    const lanes = screen.getByRole("status").querySelectorAll(".bg-lane");
    expect(lanes).toHaveLength(3);
    // A lane holds cards, so the placeholder leaves their room in it.
    expect(lanes[0]?.querySelectorAll(".shadow-card")).toHaveLength(2);
  });

  it("says the state once when one read fills two places", () => {
    render(
      <Reading message="Reading the rules" className="flex flex-col gap-6">
        <SkeletonCard lines={2} />
        <SkeletonCard lines={5} />
      </Reading>,
    );
    expect(screen.getAllByRole("status")).toHaveLength(1);
    expect(screen.getByRole("status").textContent).toBe("Reading the rules");
    // Two boxes, one sentence: the shape has no announcement of its own to add.
    expect(document.querySelectorAll(".shadow-card")).toHaveLength(2);
  });

  it("draws a conversation as turns, not as rows", () => {
    render(<SkeletonThread turns={2} message="Reading the chat" />);
    const region = screen.getByRole("status");
    // A person's message is a band of the sunken colour, and an answer is text on a measure with no box of its own.
    expect(region.querySelectorAll(".bg-sunken")).toHaveLength(2);
    expect(region.querySelectorAll(".max-w-prose")).toHaveLength(2);
    expect(region.querySelectorAll(".shadow-card")).toHaveLength(0);
  });

  it("says nothing when it is only the shape inside a place already announced", () => {
    render(<SkeletonCard lines={3} />);
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("fills its bars with a token and leaves them alone when motion is turned down", () => {
    render(<SkeletonCard lines={1} message="Reading the project" />);
    for (const bar of bars()) {
      expect(bar.className).toContain("bg-hairline");
      expect(bar.className).toContain("motion-reduce:animate-none");
    }
  });
});
