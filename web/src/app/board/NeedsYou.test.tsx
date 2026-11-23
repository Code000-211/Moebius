// @vitest-environment jsdom
/** The tinted block at the end of a thread: what the card waits for and the buttons that answer it. */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Card, InboxItem } from "../../api/types/board";
import { card } from "./cardPropertiesFixtures";
import { NeedsYouBlock } from "./NeedsYou";

afterEach(cleanup);

const item = (kind: string, data: Record<string, unknown> = {}): InboxItem => ({
  id: 1,
  kind,
  card_id: 5,
  title: "The review found a data loss",
  data,
  resolved_at: null,
  at: 0,
  assignees: [],
});

function show(waiting: InboxItem, askFor: number | null = null, extra: Partial<Card> = {}) {
  const onDecide = vi.fn();
  const onReply = vi.fn();
  const onAskAnswered = vi.fn();
  render(
    <NeedsYouBlock
      card={card(5, { number: 5, ...extra })}
      item={waiting}
      busy={false}
      askFor={askFor}
      onAskAnswered={onAskAnswered}
      onDecide={onDecide}
      onReply={onReply}
    />,
  );
  return { onDecide, onReply, onAskAnswered };
}

describe("the Moebius needs you block", () => {
  it("says what the card waits for, approves a plan and takes the person to the reply field", () => {
    const { onDecide, onReply } = show(item("approve_issue"));
    expect(screen.getByText("Moebius needs you")).toBeTruthy();
    expect(screen.getByText("The review found a data loss")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Approve" }));
    expect(onDecide).toHaveBeenCalledWith({ kind: "approve", card: 5, number: 5 });
    fireEvent.click(screen.getByRole("button", { name: "Reply" }));
    expect(onReply).toHaveBeenCalled();
  });

  it("merges, or sends the change back only with a reason", () => {
    const { onDecide, onAskAnswered } = show(item("ready_to_merge", { change: 8 }));
    fireEvent.click(screen.getByRole("button", { name: "Merge" }));
    expect(onDecide).toHaveBeenCalledWith({ kind: "merge", change: 8, number: 5 });
    fireEvent.click(screen.getByRole("button", { name: "Reject with a reason" }));
    const send = screen.getByRole("button", { name: "Send back" }) as HTMLButtonElement;
    expect(send.disabled).toBe(true);
    fireEvent.change(screen.getByLabelText("Why? The next round is told"), {
      target: { value: " Add a test " },
    });
    fireEvent.click(send);
    expect(onDecide).toHaveBeenCalledWith({
      kind: "reject",
      change: 8,
      number: 5,
      reason: "Add a test",
    });
    expect(onAskAnswered).toHaveBeenCalled();
  });

  it("asks for the reason at once when a drop sent the card back, and retries a stuck card", () => {
    show(item("ready_to_merge", { change: 8 }), 5);
    expect(screen.getByLabelText("Why? The next round is told")).toBeTruthy();
    cleanup();
    const { onDecide } = show(item("stuck"));
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(onDecide).toHaveBeenCalledWith({ kind: "retry", card: 5, number: 5 });
  });

  it("reviews a pull request someone else opened with the agent, and merges it once its checks pass", () => {
    const change = (checks: "pending" | "success" | "failure" | "none") => ({
      id: 8,
      state: "open",
      checks,
      review_verdict: null,
      outside_author: "alice",
    });
    const pending = show(item("outside_change", { change: 8 }), null, {
      change: change("pending"),
    });
    fireEvent.click(screen.getByRole("button", { name: "Review with agent" }));
    expect(pending.onDecide).toHaveBeenCalledWith({ kind: "review", change: 8, number: 5 });
    // Merging waits for the checks: the owner decides, but not before they have passed.
    expect(screen.getByRole("button", { name: "Merge" }).getAttribute("aria-disabled")).toBe(
      "true",
    );
    cleanup();
    const passed = show(item("outside_change", { change: 8 }), null, { change: change("success") });
    const merge = screen.getByRole("button", { name: "Merge" });
    expect(merge.getAttribute("aria-disabled")).toBeNull();
    fireEvent.click(merge);
    expect(passed.onDecide).toHaveBeenCalledWith({ kind: "merge", change: 8, number: 5 });
    // It is not Moebius's own change: there is nothing to send back to coding.
    expect(screen.queryByRole("button", { name: "Reject with a reason" })).toBeNull();
  });

  it("keeps the choices of the pull request when the Reviewer has read it three times", () => {
    const change = {
      id: 8,
      state: "open",
      checks: "success" as const,
      review_verdict: null,
      outside_author: "alice",
    };
    const { onDecide } = show(item("review_rounds_spent", { change: 8, author: "alice" }), null, {
      change,
    });
    fireEvent.click(screen.getByRole("button", { name: "Review with agent" }));
    expect(onDecide).toHaveBeenCalledWith({ kind: "review", change: 8, number: 5 });
    fireEvent.click(screen.getByRole("button", { name: "Merge" }));
    expect(onDecide).toHaveBeenCalledWith({ kind: "merge", change: 8, number: 5 });
  });

  it("offers the choices of the pull request when it could not be read whole", () => {
    const change = {
      id: 8,
      state: "open",
      checks: "success" as const,
      review_verdict: null,
      outside_author: "alice",
    };
    const { onDecide } = show(
      item("review_not_possible", {
        change: 8,
        author: "alice",
        reason: "it changes a binary file",
      }),
      null,
      { change },
    );
    fireEvent.click(screen.getByRole("button", { name: "Merge" }));
    expect(onDecide).toHaveBeenCalledWith({ kind: "merge", change: 8, number: 5 });
  });

  it("says it waits for the author's changes after a review that asked for them, and offers no merge", () => {
    const review = {
      round: 1,
      round_limit: 3,
      state: "posted" as const,
      verdict: "changes" as const,
      summary: "No.",
      comments: [],
      url: null,
      head: "abc",
      current: true,
    };
    const change = {
      id: 8,
      state: "open",
      checks: "success" as const,
      review_verdict: "changes" as const,
      outside_author: "alice",
      external_review: review,
    };
    show(item("outside_change", { change: 8 }), null, { change });
    expect(screen.getByTestId("waiting-for-author").textContent).toContain("Waiting for @alice");
    expect(screen.queryByRole("button", { name: "Merge" })).toBeNull();
    expect(screen.getByRole("button", { name: "Review with agent" })).toBeTruthy();
    // A push makes the review old: the choice to merge is back.
    cleanup();
    show(item("outside_change", { change: 8 }), null, {
      change: { ...change, external_review: { ...review, current: false } },
    });
    expect(screen.queryByTestId("waiting-for-author")).toBeNull();
    expect(screen.getByRole("button", { name: "Merge" })).toBeTruthy();
  });

  it("asks what to do with an issue its assignee says is done: close it, give it back, or release it", () => {
    const { onDecide } = show(
      item("done_asked", { login: "alice", reason: "The assets are cached now." }),
    );
    expect(screen.getByTestId("done-reason").textContent).toBe("The assets are cached now.");
    fireEvent.click(screen.getByRole("button", { name: "Close the issue" }));
    expect(onDecide).toHaveBeenLastCalledWith({
      kind: "done",
      card: 5,
      number: 5,
      answer: "close",
    });
    fireEvent.click(screen.getByRole("button", { name: "Give it back to @alice" }));
    expect(onDecide).toHaveBeenLastCalledWith({
      kind: "done",
      card: 5,
      number: 5,
      answer: "give_back",
    });
    fireEvent.click(screen.getByRole("button", { name: "Release it" }));
    expect(onDecide).toHaveBeenLastCalledWith({
      kind: "done",
      card: 5,
      number: 5,
      answer: "release",
    });
    // Nothing of a pull request's: no merge, no review.
    expect(screen.queryByRole("button", { name: "Merge" })).toBeNull();
  });

  it("draws no reason block when the assignee wrote nothing more", () => {
    show(item("done_asked", { login: "alice", reason: "" }));
    expect(screen.queryByTestId("done-reason")).toBeNull();
  });
});
