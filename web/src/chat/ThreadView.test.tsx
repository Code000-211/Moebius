// @vitest-environment jsdom
/**
 * The conversation is the one place on a page whose emptiness means two different things: nothing was ever said, or
 * the words have not been read yet. Only the first is worth a card with a sentence in it, so the second has to draw
 * the turns that are coming instead — a card that is about to be thrown away moves the page twice.
 */
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { ThreadView } from "./ThreadView";

afterEach(cleanup);

const nothing = { entries: [], working: false, sent: 0 };

describe("a conversation that is still being read", () => {
  it("draws the turns that are coming rather than an empty place", () => {
    render(<ThreadView {...nothing} loading loadingMessage="Reading the chat" />);
    expect(screen.getByTestId("thread-loading")).toBeTruthy();
    expect(screen.queryByTestId("empty-thread")).toBeNull();
    const region = screen.getByRole("status");
    expect(region.textContent).toBe("Reading the chat");
    // The bars are the shape of turns, and they carry no words of their own.
    for (const bar of region.querySelectorAll("[aria-hidden='true']"))
      expect(bar.textContent).toBe("");
  });

  it("says the empty place once the events have been read", () => {
    render(<ThreadView {...nothing} empty="Start with one of these." />);
    expect(screen.queryByTestId("thread-loading")).toBeNull();
    const empty = screen.getByTestId("empty-thread");
    expect(empty.textContent).toContain("Nothing said yet");
    expect(empty.textContent).toContain("Start with one of these.");
  });
});
