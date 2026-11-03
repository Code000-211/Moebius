// @vitest-environment jsdom
/**
 * The conversation is the one place on a page whose emptiness means two different things: nothing was ever said, or
 * the words have not been read yet. Only the first is worth a card with a sentence in it, so the second has to draw
 * the turns that are coming instead — a card that is about to be thrown away moves the page twice.
 */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Entry } from "./conversation";
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

  it("keeps the turns already read when the thread is read again", () => {
    // A read that is repeated while the words are on the screen is a refresh: the bars are for the first read only.
    const said: Entry = { at: 1, seq: 1, kind: "person", said: "said", message: "Hello there" };
    const { rerender } = render(
      <ThreadView {...nothing} entries={[said]} loading loadingMessage="Reading the chat" />,
    );
    expect(screen.queryByTestId("thread-loading")).toBeNull();
    rerender(<ThreadView {...nothing} entries={[said]} loadingMessage="Reading the chat" />);
    expect(screen.queryByTestId("thread-loading")).toBeNull();
  });

  it("says the empty place once the events have been read", () => {
    render(<ThreadView {...nothing} empty="Start with one of these." />);
    expect(screen.queryByTestId("thread-loading")).toBeNull();
    const empty = screen.getByTestId("empty-thread");
    expect(empty.textContent).toContain("Nothing said yet");
    expect(empty.textContent).toContain("Start with one of these.");
  });
});

/** An entry of the person, as the conversation gives it to the list. */
function said(seq: number, message = `go ${seq}`): Entry {
  return { at: 1_700_000_000 + seq, seq, kind: "person", said: "said", message };
}

describe("a conversation with more chat above it", () => {
  it("says nothing above the oldest turn while its own read is going well", () => {
    // Scrolling to the top of the conversation is what reads the rest, so the row offers no button for it: a step in
    // the middle of a gesture that already asked for the same thing is a step nobody asked for.
    const loadOlder = vi.fn();
    render(
      <ThreadView
        entries={[said(1), said(2)]}
        working={false}
        sent={0}
        hasOlder
        loadOlder={loadOlder}
      />,
    );
    expect(screen.queryByTestId("older")).toBeNull();
    expect(loadOlder).not.toHaveBeenCalled();
  });

  it("draws the shape of what is above while it is being read", () => {
    render(
      <ThreadView
        entries={[said(1)]}
        working={false}
        sent={0}
        hasOlder
        olderLoading
        loadOlder={() => {}}
      />,
    );
    const row = screen.getByTestId("older");
    expect(row.textContent).toBe("Reading earlier messages");
    for (const bar of row.querySelectorAll("[aria-hidden='true']"))
      expect(bar.textContent).toBe("");
  });

  it("gives the reason a page above could not be read, and asks again", () => {
    const loadOlder = vi.fn();
    render(
      <ThreadView
        entries={[said(1)]}
        working={false}
        sent={0}
        hasOlder
        olderError="cannot reach the server"
        loadOlder={loadOlder}
      />,
    );
    const row = screen.getByTestId("older");
    expect(row.textContent).toContain("cannot reach the server");
    fireEvent.click(screen.getByTestId("retry-older"));
    expect(loadOlder).toHaveBeenCalledOnce();
  });

  it("asks again only while there is something above to ask for", () => {
    // A chat that begins where it is drawn has nothing to bring, so the retry would change nothing and is not drawn.
    render(
      <ThreadView
        entries={[said(1)]}
        working={false}
        sent={0}
        olderError="cannot reach the server"
        loadOlder={() => {}}
      />,
    );
    expect(screen.queryByTestId("older")).toBeNull();
  });
});
