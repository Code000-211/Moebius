// @vitest-environment jsdom
/**
 * An empty chat offers a way to begin, and the offer is a beginning rather than an action: the words land in the
 * field, where the person can change them, and are not sent from under their hands.
 */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { STARTERS, Starters } from "./Starters";

afterEach(cleanup);

describe("the starters of an empty chat", () => {
  it("offers each ask as a button that says it", () => {
    render(<Starters onPick={() => {}} />);
    for (const starter of STARTERS) {
      expect(screen.getByRole("button", { name: starter })).toBeTruthy();
    }
  });

  it("hands the chosen ask to the field rather than sending it", () => {
    const picked: string[] = [];
    render(<Starters onPick={(text) => picked.push(text)} />);
    fireEvent.click(screen.getByRole("button", { name: STARTERS[0] }));
    expect(picked).toEqual([STARTERS[0]]);
  });
});
