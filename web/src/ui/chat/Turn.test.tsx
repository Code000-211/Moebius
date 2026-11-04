// @vitest-environment jsdom
/**
 * The block that says what went wrong holds the way back, when the page has one to offer: the chat whose agent went
 * away is answered again from the words it was answering, and the failure is where the person is already looking.
 */
import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { Entry } from "../../chat/conversation";
import { groupTurns } from "../../chat/turns";
import { Turn } from "./Turn";

afterEach(cleanup);

/** The turn a message that was never answered makes: the question, then why it was not answered. */
function failedTurn(): Entry[] {
  return [
    { at: 1, seq: 1, kind: "person", said: "said", message: "Broken" },
    { at: 2, seq: 2, kind: "error", source: "run", message: "the server stopped" },
  ];
}

function turnOf(entries: Entry[]) {
  const turn = groupTurns(entries, false)[0];
  if (!turn) throw new Error("these entries group into one turn");
  return turn;
}

const folding = { unfolded: new Set<string>(), onToggle: () => {} };

describe("a turn that ended in a failure", () => {
  it("puts the way back beside the words that say what went wrong", () => {
    render(
      <Turn
        turn={turnOf(failedTurn())}
        {...folding}
        retry={<button type="button">Retry</button>}
      />,
    );
    const alert = screen.getByRole("alert");
    expect(alert.textContent).toContain("the server stopped");
    expect(within(alert).getByRole("button", { name: "Retry" })).toBeTruthy();
    // The way back is the one thing to press there, so the block carries no Copy of its own.
    expect(within(alert).queryByRole("button", { name: "Copy error" })).toBeNull();
  });

  it("says only what went wrong where there is nothing to press", () => {
    render(<Turn turn={turnOf(failedTurn())} {...folding} />);
    const alert = screen.getByRole("alert");
    expect(within(alert).queryByRole("button", { name: "Retry" })).toBeNull();
    expect(within(alert).getByRole("button", { name: "Copy error" })).toBeTruthy();
  });

  it("offers nothing on a turn that was answered", () => {
    const answered: Entry[] = [
      { at: 1, seq: 1, kind: "person", said: "said", message: "Broken" },
      { at: 2, seq: 2, kind: "text", text: "Here is what happened." },
    ];
    render(
      <Turn turn={turnOf(answered)} {...folding} retry={<button type="button">Retry</button>} />,
    );
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.queryAllByRole("button", { name: "Retry" })).toHaveLength(0);
  });
});
