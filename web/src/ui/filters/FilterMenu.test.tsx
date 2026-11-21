import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { FilterMenu } from "./FilterMenu";

const fields = [
  { key: "agent", label: "Agent", options: [["Coder", "Coder"]] as Array<[string, string]> },
];

describe("the header's Filter menu", () => {
  it("reads Filter with nothing set, and Filter · n with filters set", () => {
    const { rerender } = render(
      <FilterMenu fields={fields} values={{}} onFilter={() => {}} onClear={() => {}} />,
    );
    expect(screen.getByRole("button", { name: "Filter" })).toBeTruthy();
    rerender(
      <FilterMenu
        fields={fields}
        values={{ agent: "Coder" }}
        onFilter={() => {}}
        onClear={() => {}}
      />,
    );
    expect(screen.getByRole("button", { name: "Filter · 1" })).toBeTruthy();
  });

  it("offers Clear filters only while a filter is set", async () => {
    const onClear = vi.fn();
    render(
      <FilterMenu
        fields={fields}
        values={{ agent: "Coder" }}
        onFilter={() => {}}
        onClear={onClear}
      />,
    );
    fireEvent.pointerDown(screen.getByRole("button", { name: "Filter · 1" }), {
      button: 0,
      ctrlKey: false,
    });
    fireEvent.click(await screen.findByRole("menuitem", { name: "Clear filters" }));
    expect(onClear).toHaveBeenCalled();
  });
});
