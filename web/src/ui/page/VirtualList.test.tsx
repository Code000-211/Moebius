// @vitest-environment jsdom
/**
 * The list of rows: it draws a few of many, says where each stands, moves the choice by key, and keeps the row at the
 * standard's height with no shadow.
 */
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { createRef, useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ListRow } from "./ListRow";
import { type ListItem, VirtualList, type VirtualListHandle } from "./VirtualList";
import { mockViewport } from "./virtualListTest";

let restore: () => void;
beforeEach(() => {
  restore = mockViewport({ height: 600 });
});
afterEach(() => {
  cleanup();
  restore();
});

interface Issue {
  id: string;
  title: string;
}

function issues(count: number, groups = false): ListItem<Issue>[] {
  const items: ListItem<Issue>[] = [];
  for (let n = 0; n < count; n++) {
    if (groups && n % 5 === 0) items.push({ type: "group", key: `g${n}`, name: `Group ${n / 5}` });
    items.push({ type: "row", key: `k${n}`, data: { id: `MOE-${n}`, title: `Issue ${n}` } });
  }
  return items;
}

function Harness({
  items,
  onOpen,
  onRange,
  handle,
}: {
  items: ListItem<Issue>[];
  onOpen?: (key: string) => void;
  onRange?: (keys: string[]) => void;
  handle?: React.Ref<VirtualListHandle>;
}) {
  const [selected, setSelected] = useState<string | null>(null);
  return (
    <VirtualList
      label="Issues"
      items={items}
      selectedKey={selected}
      onSelect={setSelected}
      onSelectRange={onRange}
      onOpen={onOpen}
      handle={handle}
      renderRow={(issue, { at, selected: on, onClick }) => (
        <ListRow
          at={at}
          selected={on}
          onClick={onClick}
          status="running"
          id={issue.id}
          title={issue.title}
        />
      )}
    />
  );
}

describe("a virtual list", () => {
  it("is a list of list items, each saying its place among all of them", () => {
    render(<Harness items={issues(20, true)} />);
    const list = screen.getByRole("list", { name: "Issues" });
    const shown = within(list).getAllByRole("listitem");
    expect(shown.length).toBeGreaterThan(0);
    // 20 rows and 4 group rows.
    expect(shown[0]?.getAttribute("aria-posinset")).toBe("1");
    expect(shown[0]?.getAttribute("aria-setsize")).toBe("24");
    expect(shown[1]?.getAttribute("aria-posinset")).toBe("2");
    expect(shown[0]?.textContent).toBe("Group 0");
  });

  it("gives a row the height class of the standard and no shadow", () => {
    render(<Harness items={issues(3)} />);
    const row = screen.getAllByTestId("list-row")[0];
    expect(row?.classList.contains("h-9")).toBe(true);
    expect(Array.from(row?.classList ?? []).some((name) => name.startsWith("shadow"))).toBe(false);
  });

  it("draws a few of 2000 rows, and the last one once the choice is moved there", () => {
    render(<Harness items={issues(2000)} />);
    const drawn = () => screen.queryAllByTestId("list-row").length;
    expect(drawn()).toBeGreaterThan(0);
    expect(drawn()).toBeLessThan(100);
    expect(screen.queryByText("Issue 1999")).toBeNull();
    const list = screen.getByRole("list", { name: "Issues" });
    list.focus();
    fireEvent.keyDown(list, { key: "End" });
    expect(screen.getByText("Issue 1999")).toBeTruthy();
    expect(drawn()).toBeLessThan(100);
  });

  it("moves the choice with the arrows, skipping group rows, and opens it with Enter", () => {
    const open = vi.fn();
    render(<Harness items={issues(10, true)} onOpen={open} />);
    const list = screen.getByRole("list", { name: "Issues" });
    fireEvent.keyDown(list, { key: "ArrowDown" });
    expect(
      screen.getByText("Issue 0").closest("[role=listitem]")?.getAttribute("aria-current"),
    ).toBe("true");
    fireEvent.keyDown(list, { key: "ArrowDown" });
    expect(
      screen.getByText("Issue 1").closest("[role=listitem]")?.getAttribute("aria-current"),
    ).toBe("true");
    fireEvent.keyDown(list, { key: "ArrowUp" });
    expect(
      screen.getByText("Issue 0").closest("[role=listitem]")?.getAttribute("aria-current"),
    ).toBe("true");
    fireEvent.keyDown(list, { key: "Enter" });
    expect(open).toHaveBeenCalledWith("k0");
    // The group row between Issue 4 and Issue 5 is stepped over.
    for (let n = 0; n < 5; n++) fireEvent.keyDown(list, { key: "ArrowDown" });
    expect(
      screen.getByText("Issue 5").closest("[role=listitem]")?.getAttribute("aria-current"),
    ).toBe("true");
  });

  it("chooses a range from the ordered keys, beyond what is drawn", () => {
    const range = vi.fn();
    render(<Harness items={issues(2000)} onRange={range} />);
    const list = screen.getByRole("list", { name: "Issues" });
    fireEvent.keyDown(list, { key: "ArrowDown" });
    fireEvent.keyDown(list, { key: "End", shiftKey: true });
    const keys = range.mock.calls.at(-1)?.[0] as string[];
    expect(keys).toHaveLength(2000);
    expect(keys[0]).toBe("k0");
    expect(keys.at(-1)).toBe("k1999");
  });

  it("selects on a click and takes a range on a shift-click", () => {
    const range = vi.fn();
    render(<Harness items={issues(10)} onRange={range} />);
    fireEvent.click(screen.getByText("Issue 2"));
    expect(
      screen.getByText("Issue 2").closest("[role=listitem]")?.getAttribute("aria-current"),
    ).toBe("true");
    fireEvent.click(screen.getByText("Issue 5"), { shiftKey: true });
    expect(range).toHaveBeenCalledWith(["k2", "k3", "k4", "k5"]);
  });

  it("scrolls to a key it is asked for", () => {
    const handle = createRef<VirtualListHandle>();
    render(<Harness items={issues(2000)} handle={handle} />);
    act(() => handle.current?.scrollToKey("k1999"));
    expect(screen.getByText("Issue 1999")).toBeTruthy();
  });

  it("draws skeleton rows while loading and the empty slot when there is nothing", () => {
    const { rerender } = render(
      <VirtualList
        label="x"
        items={[]}
        loading
        renderRow={() => null}
        empty={<p>Nothing needs you</p>}
      />,
    );
    expect(document.querySelectorAll("[data-skeleton-row]").length).toBeGreaterThan(0);
    expect(screen.queryByText("Nothing needs you")).toBeNull();
    rerender(
      <VirtualList label="x" items={[]} renderRow={() => null} empty={<p>Nothing needs you</p>} />,
    );
    expect(screen.getByText("Nothing needs you")).toBeTruthy();
  });
});

describe("a row", () => {
  it("shows what it is given, the avatar only for a person, and the edge only when selected", () => {
    const { rerender } = render(
      <ListRow
        status="needsYou"
        id="MOE-1"
        title="A title"
        reason="Plan ready"
        age="2m"
        action={<button type="button">Approve plan</button>}
        owner="leon"
      />,
    );
    expect(screen.getByText("MOE-1")).toBeTruthy();
    expect(screen.getByText("Plan ready")).toBeTruthy();
    expect(screen.getByText("2m")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Approve plan" })).toBeTruthy();
    expect(screen.getByTitle("@leon")).toBeTruthy();
    expect(screen.getByTestId("list-row").className).not.toContain("before:bg-accent");
    rerender(<ListRow status="running" id="MOE-2" title="Run" selected />);
    expect(screen.queryByTitle("@leon")).toBeNull();
    expect(screen.getByTestId("list-row").className).toContain("before:bg-accent");
  });
});
