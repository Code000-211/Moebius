// @vitest-environment jsdom
/** A row of figures: the value and its label, the hint as a tooltip, and tabular numerals on every value. */
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { Figures } from "./Figures";

afterEach(cleanup);

describe("a row of figures", () => {
  it("shows each value with its label and keeps the hint as a tooltip", () => {
    render(<Figures items={[{ label: "Cards done", value: "42", hint: "7 points" }]} />);
    const item = screen.getByTestId("Cards done");
    expect(item.textContent).toBe("42Cards done");
    expect(item.getAttribute("title")).toBe("7 points");
    expect(screen.getAllByRole("listitem")).toHaveLength(1);
  });

  it("sets the values in tabular numerals", () => {
    const { container } = render(<Figures items={[{ label: "Model calls", value: "1,284" }]} />);
    expect(container.querySelector("[data-figure]")?.className).toContain("tabular-nums");
  });
});
