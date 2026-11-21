// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { InboxRow } from "./InboxRow";
import type { Entry } from "./model";

const entry: Entry = {
  key: "item-7",
  status: "needsYou",
  id: "WEB-24",
  title: "Stop the flaky test is stuck",
  reason: "Stuck: the round limit is reached",
  owner: "leon",
  at: 100,
  item: null,
  card: 3,
  approves: null,
};

function row(extra: Partial<Parameters<typeof InboxRow>[0]> = {}) {
  const props = {
    entry,
    now: 220,
    at: { index: 1, size: 1 },
    selected: false,
    approving: false,
    onClick: vi.fn(),
    onApprove: vi.fn(),
    ...extra,
  };
  render(<InboxRow {...props} />);
  return props;
}

afterEach(cleanup);

describe("an inbox row", () => {
  it("says the key, the title, one reason, the owner and the age in the compact way", () => {
    row();
    expect(screen.getByText("WEB-24")).toBeTruthy();
    expect(screen.getByText("Stop the flaky test is stuck")).toBeTruthy();
    expect(screen.getByText("Stuck: the round limit is reached")).toBeTruthy();
    expect(screen.getByText("2m")).toBeTruthy();
    expect(screen.getByTitle("@leon")).toBeTruthy();
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("has no unread dot", () => {
    row();
    expect(screen.queryByTestId("unread-dot")).toBeNull();
  });

  it("leaves no gap for the id of a row that has none", () => {
    row({ entry: { ...entry, id: "" } });
    expect(document.querySelector(".font-mono")).toBeNull();
  });

  it("is chosen with a click", () => {
    const { onClick } = row();
    fireEvent.click(screen.getByTestId("inbox-item"));
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("approves from its own button without choosing the row", () => {
    const { onClick, onApprove } = row({ entry: { ...entry, approves: 3 } });
    fireEvent.click(screen.getByRole("button", { name: "Approve" }));
    expect(onApprove).toHaveBeenCalledWith(3);
    expect(onClick).not.toHaveBeenCalled();
  });
});
