// @vitest-environment jsdom
/** The review of a pull request someone else opened, in the card window: the verdict, the summary and the line comments. */
import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { Measurement, ExternalReview as Round } from "../../api/types/board";
import { ExternalReview } from "./ExternalReview";

afterEach(cleanup);

const round: Round = {
  round: 2,
  kind: "author" as const,
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

describe("the measurement of a followed pull request", () => {
  const measurement: Measurement = {
    measurer: "bob",
    head: "abcdef0123456789",
    before: "0123456789abcdef",
    state: "submitted",
    current: true,
    accepted: false,
    at: 1,
  };
  const measured: Round = { ...round, kind: "measurement", state: "wanted", verdict: null };

  it("says the measurement waits for the checks, then that the Reviewer is reading it", () => {
    render(<ExternalReview review={measured} forge="GitHub" measurement={measurement} />);
    expect(screen.getByTestId("external-review").textContent).toContain(
      "reads @bob's measurement once the checks have passed",
    );
    cleanup();
    render(
      <ExternalReview
        review={{ ...measured, state: "running" }}
        forge="GitHub"
        measurement={measurement}
      />,
    );
    expect(screen.getByTestId("external-review").textContent).toContain("with @bob's measurement");
  });

  it("says accepted, with the measurer and the commit, once the Reviewer approved it", () => {
    render(
      <ExternalReview
        review={{ ...measured, state: "merged", verdict: "approve", summary: "Meets the bar." }}
        forge="GitHub"
        measurement={{ ...measurement, state: "accepted", accepted: true }}
      />,
    );
    const note = screen.getByTestId("measurement-note");
    expect(note.textContent).toBe("Measurement by @bob accepted for abcdef0.");
    expect(note.getAttribute("data-state")).toBe("accepted");
  });

  it("says the author's committed measurement was accepted with the Reviewer's approval", () => {
    render(
      <ExternalReview
        review={{ ...round, state: "merged", verdict: "approve", summary: "Meets the brief." }}
        forge="GitHub"
        authorMeasurement="alice"
      />,
    );
    const note = screen.getByTestId("measurement-note");
    expect(note.textContent).toContain(
      "@alice, the author's, committed with the change and reviewed",
    );
    expect(note.getAttribute("data-state")).toBe("author");
  });

  it("says not enough when the Reviewer still wants more, and where to read what", () => {
    render(
      <ExternalReview
        review={{
          ...measured,
          state: "awaiting_measurement",
          verdict: "approve_code",
          summary: "Only one GPU.",
        }}
        forge="GitHub"
        measurement={{ ...measurement, state: "insufficient" }}
      />,
    );
    expect(screen.getByTestId("measurement-note").textContent).toContain("is not enough");
    expect(screen.getByTestId("external-review").textContent).toContain("Only one GPU.");
  });

  it("says a measurement of an earlier commit no longer counts", () => {
    render(
      <ExternalReview
        review={{ ...round, state: "awaiting_measurement", verdict: "approve_code" }}
        forge="GitHub"
        measurement={{ ...measurement, state: "invalid", current: false }}
      />,
    );
    const note = screen.getByTestId("measurement-note").textContent ?? "";
    expect(note).toContain("no longer counts");
    expect(note).toContain("abcdef0");
  });
});
