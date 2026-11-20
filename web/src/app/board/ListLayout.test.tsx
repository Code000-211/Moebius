// @vitest-environment jsdom
/**
 * The list layout of the board: rows with a status icon, the mono number, the title and the age and no column
 * headers; a group row named by its column; a thousand cards drawn as a few rows; a click opens the card; the select
 * box and the right click are kept.
 */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { mockViewport } from "../../ui/page/virtualListTest";
import { board, card } from "./cardPropertiesFixtures";
import { ListLayout } from "./ListLayout";

let restore: () => void;
beforeEach(() => {
  restore = mockViewport({ height: 600 });
});
afterEach(() => {
  cleanup();
  restore();
});

function draw(count: number, extra: Partial<Parameters<typeof ListLayout>[0]> = {}) {
  const cards = Array.from({ length: count }, (_, n) =>
    card(n + 1, { title: `Issue ${n + 1}`, column: n % 2 === 0 ? "Not started" : "Done" }),
  );
  return render(
    <ListLayout
      board={board(cards)}
      inbox={[]}
      groups={[
        {
          key: "Not started",
          name: "Not started",
          cards: cards.filter((c) => c.column !== "Done"),
        },
        { key: "Done", name: "Done", cards: cards.filter((c) => c.column === "Done") },
      ]}
      hidden={new Set()}
      inspectedId={undefined}
      onSelect={() => {}}
      initialItemCount={30}
      {...extra}
    />,
  );
}

it("shows the status icon, the number, the title and the age of a card, under a group row named by its column", () => {
  draw(4);
  const groups = screen.getAllByTestId("group-row").map((row) => row.textContent);
  expect(groups).toEqual(["Not started", "Done"]);
  const row = screen.getAllByTestId("list-row")[0] as HTMLElement;
  expect(row.textContent).toContain("Issue 1");
  expect(row.querySelector("[data-testid^=status-]")).toBeTruthy();
  expect(row.querySelector(".font-mono")?.textContent).toBeTruthy();
  expect(screen.queryAllByRole("columnheader")).toHaveLength(0);
});

it("draws fewer than a hundred rows for two thousand cards", () => {
  draw(2000, { initialItemCount: undefined });
  const drawn = screen.getAllByTestId("list-row").length;
  expect(drawn).toBeGreaterThan(0);
  expect(drawn).toBeLessThan(100);
});

it("opens the card of a row that is clicked", () => {
  const onSelect = vi.fn();
  draw(4, { onSelect });
  fireEvent.click(screen.getByText("Issue 2"));
  expect(onSelect).toHaveBeenCalledWith(expect.objectContaining({ id: 2 }));
});

it("puts the select box in the row and the right click on the row", () => {
  const onContextMenu = vi.fn(() => () => {});
  draw(2, {
    select: (c) => <input type="checkbox" aria-label={`Choose #${c.number}`} />,
    onContextMenu,
  });
  expect(screen.getByLabelText("Choose #1")).toBeTruthy();
  fireEvent.contextMenu(screen.getByText("Issue 1"));
  expect(onContextMenu).toHaveBeenCalled();
});
