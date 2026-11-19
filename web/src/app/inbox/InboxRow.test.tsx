import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { InboxItem, Snapshot } from "../../api/types/board";
import { InboxRow } from "./InboxRow";

const snapshot: Snapshot = { boards: [], harness_changes: [], inbox: [] };
const item: InboxItem = {
  id: 7,
  kind: "stuck",
  card_id: null,
  title: "Stop the flaky test is stuck",
  data: { reason: "the round limit is reached" },
  resolved_at: null,
  at: 100,
};

function row(unread: boolean, onChoose = () => {}) {
  render(
    <InboxRow
      item={item}
      snapshot={snapshot}
      now={160}
      chosen={false}
      unread={unread}
      onChoose={onChoose}
    />,
  );
}

describe("an inbox row", () => {
  it("says the kind and why in a grey line under the title, and has no button but itself", () => {
    row(true);
    expect(screen.getByText("Stop the flaky test is stuck")).toBeTruthy();
    expect(screen.getByText("Stuck", { exact: true })).toBeTruthy();
    expect(screen.getByText(/Stopped: the round limit is reached\./)).toBeTruthy();
    expect(screen.getAllByRole("button")).toHaveLength(1);
  });

  it("has a dot while unread", () => {
    row(true);
    expect(screen.getByTestId("unread-dot")).toBeTruthy();
  });

  it("has no dot when read", () => {
    row(false);
    expect(screen.queryByTestId("unread-dot")).toBeNull();
  });

  it("is chosen with a click", () => {
    const onChoose = vi.fn();
    row(false, onChoose);
    fireEvent.click(screen.getByTestId("inbox-item"));
    expect(onChoose).toHaveBeenCalledOnce();
  });
});
