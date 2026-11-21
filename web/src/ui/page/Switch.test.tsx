// @vitest-environment jsdom
/**
 * A switch says its state by where the knob is and by `aria-checked` (it has no word beside it), and takes one
 * press to change it, where two buttons offering "On" and "Off" took two and looked like a choice of actions.
 */
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { Switch } from "./Switch";

afterEach(cleanup);

describe("a switch", () => {
  it("is a switch that says what it switches", () => {
    render(<Switch checked={false} onChange={() => {}} label="Runs on its schedule" />);
    const control = screen.getByRole("switch", { name: "Runs on its schedule" });
    expect(control.getAttribute("aria-checked")).toBe("false");
    expect(screen.queryByText("Off")).toBeNull();
  });

  it("hands the other state to the page when it is pressed", () => {
    const seen: boolean[] = [];
    render(
      <Switch checked={false} onChange={(next) => seen.push(next)} label="Runs on its schedule" />,
    );
    fireEvent.click(screen.getByRole("switch"));
    expect(seen).toEqual([true]);
  });

  it("is read as on by a screen reader, with no word beside it", () => {
    render(<Switch checked onChange={() => {}} label="Runs on its schedule" />);
    const control = screen.getByRole("switch");
    expect(control.getAttribute("aria-checked")).toBe("true");
    expect(screen.queryByText("On")).toBeNull();
    expect(control.textContent).toBe("");
    expect(screen.getByRole("switch", { name: "Runs on its schedule", checked: true })).toBe(
      control,
    );
  });

  it("is at least as tall as every other control", () => {
    render(<Switch checked={false} onChange={() => {}} label="Runs on its schedule" />);
    expect(screen.getByRole("switch").getAttribute("class")).toContain("min-h-7");
  });
});
