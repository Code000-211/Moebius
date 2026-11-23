// @vitest-environment jsdom
/** The review of a pull request someone else opened, in the card window: the verdict, the summary and the line comments. */
import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { ExternalReview as Round } from "../../api/types/board";
import { ExternalReview } from "./ExternalReview";

afterEach(cleanup);

const round: Round = {
  round: 2,
  round_limit: 3,
  state: "posted",
  verdict: "changes",
  summary: "One bug, one missing test.",
  comments: [
    { path: "src/cache.rs", line: 42, body: "This grows without a bound." },
    { path: "src/cache.rs", line: 57, body: "Add a test." },
  ],
  url: "https://github.test/acme/demo/pull/7#pullrequestreview-91",
  head: "abc",
  current: true,
};

describe("the review of a followed pull request", () => {
  it("shows the verdict, the round, the summary and each comment under its file and line", () => {
    render(<ExternalReview review={round} forge="GitHub" />);
    const block = screen.getByTestId("external-review");
    expect(within(block).getByText("Changes requested")).toBeTruthy();
    expect(within(block).getByText("Round 2 of 3")).toBeTruthy();
    expect(within(block).getByText("One bug, one missing test.")).toBeTruthy();
    const boxes = screen.getAllByTestId("review-comment");
    expect(boxes).toHaveLength(2);
    expect(boxes[0]?.textContent).toContain("src/cache.rs line 42");
    expect(boxes[0]?.textContent).toContain("This grows without a bound.");
    const link = screen.getByRole("link", { name: "Open on GitHub" });
    expect(link.getAttribute("href")).toBe(round.url);
    expect(link.getAttribute("rel")).toContain("noopener");
  });

  it("says approved, and merged once it is, and names the forge it links to", () => {
    const approved: Round = { ...round, verdict: "approve", comments: [], summary: "Fine." };
    const { rerender } = render(<ExternalReview review={approved} forge="Forgejo" />);
    expect(screen.getByText("Approved")).toBeTruthy();
    expect(screen.queryByTestId("review-comment")).toBeNull();
    expect(screen.getByRole("link", { name: "Open on Forgejo" })).toBeTruthy();
    rerender(<ExternalReview review={{ ...approved, state: "merged" }} forge="Forgejo" />);
    expect(screen.getByText("Approved and merged")).toBeTruthy();
  });

  it("says when it read an earlier version, and has no link before the review is on the forge", () => {
    render(<ExternalReview review={{ ...round, current: false, url: null }} forge="GitHub" />);
    expect(screen.getByText("Read an earlier version of the pull request")).toBeTruthy();
    expect(screen.queryByRole("link")).toBeNull();
  });

  it("says one quiet line while the review waits for the checks or runs, and when it gave nothing", () => {
    const quiet = (state: Round["state"], verdict: Round["verdict"] = null) => {
      cleanup();
      render(<ExternalReview review={{ ...round, state, verdict }} forge="GitHub" />);
      return screen.getByTestId("external-review").textContent;
    };
    expect(quiet("wanted")).toBe("Review starts once the checks have passed");
    expect(quiet("running")).toBe("Reviewing the pull request");
    expect(quiet("failed")).toBe("The last review gave no result");
    expect(quiet("stale")).toBe("The last review was dropped");
  });
});
