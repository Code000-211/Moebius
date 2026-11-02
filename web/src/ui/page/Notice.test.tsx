// @vitest-environment jsdom
/**
 * A notice carries its own action, so a page says a result once: the message and the way out of it are one thing on
 * the screen, rather than a notice wrapped in a row built to hang a "Dismiss" beside it.
 */
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Notice } from "./Notice";

afterEach(cleanup);

describe("a notice", () => {
  it("holds its own action, beside the words it answers", () => {
    const dismiss = vi.fn();
    render(
      <Notice
        tone="error"
        action={
          <button type="button" onClick={dismiss}>
            Dismiss
          </button>
        }
      >
        Cannot move #3 to Done: Nobody decides this card now
      </Notice>,
    );
    const line = screen.getByRole("alert");
    expect(line.textContent).toContain("Nobody decides this card now");
    fireEvent.click(within(line).getByRole("button", { name: "Dismiss" }));
    expect(dismiss).toHaveBeenCalledTimes(1);
  });

  it("gives its words a box of their own, so a long refusal wraps instead of pushing the sheet wider", () => {
    render(
      <Notice tone="error">travel-timeline needs the forge github, which is not set up</Notice>,
    );
    const line = screen.getByRole("alert");
    const words = within(line).getByText(
      "travel-timeline needs the forge github, which is not set up",
    );
    expect(words.tagName).toBe("SPAN");
    expect(words.className).toContain("break-words");
    expect(words.className).toContain("min-w-0");
  });

  it("is the live message its tone promises", () => {
    render(<Notice>Saved as a commit on the default branch.</Notice>);
    expect(screen.getByRole("status").textContent).toBe("Saved as a commit on the default branch.");
  });
});
