// @vitest-environment jsdom
/** The notice of a frozen repository: the reason and where the freeze came from. */
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { ExternalReview } from "./ExternalReview";
import { FrozenNotice } from "./FrozenNotice";

afterEach(cleanup);

describe("the frozen notice", () => {
  it("says Frozen with the reason and who froze it", () => {
    render(
      <FrozenNotice frozen={{ reason: "two reverts in 7 days", source: "automatic", since: 5 }} />,
    );
    const text = screen.getByTestId("frozen-notice").textContent ?? "";
    expect(text).toContain("Frozen: two reverts in 7 days");
    expect(text).toContain("A breaker froze it.");
    expect(text).toContain("until a person unfreezes it");
  });

  it("is nothing for a repository that is not frozen", () => {
    const { container } = render(<FrozenNotice frozen={null} />);
    expect(container.textContent).toBe("");
  });
});

describe("a review whose code is approved and whose measurement is missing", () => {
  it("says so in plain words and names what starts the next round", () => {
    render(
      <ExternalReview
        forge="GitHub"
        review={{
          round: 1,
          round_limit: 3,
          state: "awaiting_measurement",
          verdict: "approve_code",
          summary: "The kernel change is right.",
          comments: [],
          url: null,
          head: "abc",
          current: true,
        }}
      />,
    );
    const review = screen.getByTestId("external-review");
    expect(review.textContent).toContain("Code approved, measurement needed");
    expect(review.textContent).not.toContain("Approved and merged");
    expect(screen.getByTestId("measurement-needed").textContent).toContain("## Measurement");
    expect(review.textContent).toContain("The kernel change is right.");
  });
});
