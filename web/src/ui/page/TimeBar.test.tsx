// @vitest-environment jsdom
/**
 * A time bar spans its dates on a shared axis, fills as far as its progress, marks today only where today falls
 * inside the axis, and says its progress to a screen reader.
 */
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { axisOf, placeOnAxis, TimeBar } from "./TimeBar";

afterEach(cleanup);

const axis = { start: "2026-11-01", end: "2026-11-10" };

describe("placeOnAxis", () => {
  it("puts a span at its first and last day, both counted", () => {
    expect(placeOnAxis("2026-11-03", "2026-11-07", axis)).toEqual({ left: 20, width: 50 });
    expect(placeOnAxis("2026-11-01", "2026-11-10", axis)).toEqual({ left: 0, width: 100 });
  });
  it("keeps a span that runs past the axis inside it", () => {
    expect(placeOnAxis("2026-10-20", "2026-11-02", axis)).toEqual({ left: 0, width: 20 });
    expect(placeOnAxis("2026-11-09", "2026-12-30", axis)).toEqual({ left: 80, width: 20 });
  });
});

describe("axisOf", () => {
  it("runs from the earliest start to the latest end", () => {
    expect(
      axisOf([
        { start: "2026-11-05", end: "2026-11-09" },
        { start: "2026-11-01", end: "2026-11-03" },
      ]),
    ).toEqual({ start: "2026-11-01", end: "2026-11-09" });
    expect(axisOf([])).toBeNull();
  });
});

describe("TimeBar", () => {
  it("spans its dates and fills as far as its progress", () => {
    render(
      <TimeBar start="2026-11-03" end="2026-11-07" axis={axis} progress={40} label="2 of 5" />,
    );
    const bar = screen.getByRole("progressbar", { name: "2 of 5" });
    expect(bar.getAttribute("aria-valuenow")).toBe("40");
    expect(bar.style.left).toBe("20%");
    expect(bar.style.width).toBe("50%");
    expect((bar.firstElementChild as HTMLElement).style.width).toBe("40%");
  });
  it("marks today inside the axis only", () => {
    const { rerender } = render(
      <TimeBar start="2026-11-03" end="2026-11-07" axis={axis} progress={0} today="2026-11-05" />,
    );
    expect(screen.getByTestId("time-bar-today")).toBeTruthy();
    rerender(
      <TimeBar start="2026-11-03" end="2026-11-07" axis={axis} progress={0} today="2026-12-05" />,
    );
    expect(screen.queryByTestId("time-bar-today")).toBeNull();
  });
});
