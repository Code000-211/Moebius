// @vitest-environment jsdom
/** The tinted block at the end of a thread: what the card waits for and the buttons that answer it. */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { InboxItem } from "../../api/types/board";
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

function show(waiting: InboxItem, askFor: number | null = null) {
  const onDecide = vi.fn();
  const onReply = vi.fn();
  const onAskAnswered = vi.fn();
  render(
    <NeedsYouBlock
      card={card(5, { number: 5 })}
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
});
