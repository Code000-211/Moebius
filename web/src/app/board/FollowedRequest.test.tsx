// @vitest-environment jsdom
/** What the card window says of someone else's work: the pull request the card follows, or the issue they took. */
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { Change } from "../../api/types/board";
import { card, board as plainBoard } from "./cardPropertiesFixtures";

/** A board of a repository on GitHub. */
const board = (...args: Parameters<typeof plainBoard>) => {
  const made = plainBoard(...args);
  return {
    ...made,
    repository: { ...made.repository, forge: "github" as const, github: "acme/demo" },
  };
};

import { FollowedRequest } from "./FollowedRequest";

afterEach(cleanup);

const change: Change = {
  id: 9,
  repository_id: 1,
  harness_repository: false,
  card_id: 5,
  branch: "moebius/pr-7",
  state: "open",
  pull_request: 7,
  checks: "success",
  review: null,
  head: "abc",
  published_at: 1,
  outside_author: "alice",
  taken_from: "In progress",
};

describe("what the window says of someone else's work", () => {
  it("shows a followed pull request as an own change is shown: badge, who opened it, its checks", () => {
    const followed = card(5, {
      column: "In review",
      taken_by: "alice",
      change: {
        id: 9,
        state: "open",
        checks: "success",
        review_verdict: null,
        outside_author: "alice",
      },
    });
    const full = { ...board([followed]), changes: [change] };
    render(<FollowedRequest board={full} card={followed} />);
    expect(screen.getByLabelText("Pull request #7, open")).toBeTruthy();
    expect(screen.getByText("by @alice")).toBeTruthy();
    expect(screen.getByText(/Checks: /)).toBeTruthy();
    // Nothing of the issue's own "taken" sentence: the pull request is what there is to say.
    expect(screen.queryByTestId("taken-note")).toBeNull();
  });

  it("says who took an issue, and why the server could not assign its own person", () => {
    const taken = card(5, {
      taken_by: "alice",
      assign_problem: "Set your login in Settings > You > Profile.",
    });
    render(<FollowedRequest board={board([taken])} card={taken} />);
    expect(screen.getByTestId("taken-note").textContent).toContain("Taken by @alice on GitHub");
    expect(screen.getByRole("alert").textContent).toContain("Settings > You > Profile");
  });

  it("draws nothing for a card that is this server's own", () => {
    const own = card(5);
    const { container } = render(<FollowedRequest board={board([own])} card={own} />);
    expect(container.textContent).toBe("");
  });
});
