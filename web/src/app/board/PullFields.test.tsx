// @vitest-environment jsdom
/** What the card window says of a pull request's body, read by the pull request template (ADR 0034). */
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { CardChange, PullFields as Fields } from "../../api/types/board";
import { headingOf, PullFields } from "./PullFields";

afterEach(cleanup);

const pull: Fields = {
  fixes: [12],
  fields: {
    item: "K-4: cache the plan",
    what_the_change_does: "Plans are cached.",
    tests_i_ran: "- [x] `cargo test`: 412 passed",
    measurement: "measured: 4.2% faster",
    rollback_plan: "Revert the commit.",
  },
  tests: [
    { done: true, text: "`cargo test`: 412 passed" },
    { done: false, text: "`cargo bench`: not run" },
  ],
  measurement: { state: "measured", reason: "4.2% faster" },
  missing: [],
};

const change = (extra: Partial<CardChange> = {}): CardChange => ({
  id: 1,
  state: "open",
  checks: "success",
  review_verdict: null,
  pull,
  ...extra,
});

describe("the fields of a pull request body", () => {
  it("shows each section under its heading, the tests as a checklist and the measurement as a state", () => {
    render(<PullFields change={change()} />);
    expect(screen.getByText("Fixes #12")).toBeTruthy();
    expect(screen.getByText("What the change does")).toBeTruthy();
    expect(screen.getByText("Plans are cached.")).toBeTruthy();
    // A section of the project's own is shown too, under its heading.
    expect(screen.getByText("Rollback plan")).toBeTruthy();
    expect(screen.getByTestId("pull-tests").textContent).toContain("412 passed");
    expect(screen.getByTestId("pull-tests").textContent).toContain("not run");
    expect(screen.getByTestId("pull-measurement").textContent).toBe("measured: 4.2% faster");
    expect(screen.queryByTestId("pull-missing")).toBeNull();
    expect(screen.queryByTestId("pull-draft")).toBeNull();
  });

  it("says Draft while the pull request is one, and what the body lacks", () => {
    render(
      <PullFields
        change={change({
          draft: true,
          pull: { ...pull, measurement: null, missing: ["Measurement", "Tests I ran"] },
        })}
      />,
    );
    expect(screen.getByTestId("pull-draft").textContent).toBe("Draft");
    expect(screen.getByTestId("pull-missing").textContent).toBe(
      "The body lacks: Measurement, Tests I ran.",
    );
    expect(screen.queryByTestId("pull-measurement")).toBeNull();
  });

  it("shows nothing until the body was read, or for a card without a change", () => {
    const { container } = render(<PullFields change={change({ pull: undefined })} />);
    expect(container.textContent).toBe("");
    cleanup();
    expect(render(<PullFields change={null} />).container.textContent).toBe("");
  });

  it("turns a slug back into its heading", () => {
    expect(headingOf("what_the_change_does")).toBe("What the change does");
    expect(headingOf("item")).toBe("Item");
  });
});
