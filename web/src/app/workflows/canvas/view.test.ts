import { describe, expect, it } from "vitest";
import { CARD_WIDTH, READABLE_CARD_PX, READABLE_ZOOM, readableViewport } from "./view";

const pane = { width: 900, height: 600 };

describe("the view a graph opens at", () => {
  it("shows a box at least as wide as the readable floor", () => {
    expect(CARD_WIDTH * READABLE_ZOOM).toBeCloseTo(READABLE_CARD_PX);
  });

  it("fits a small graph whole, centred, and never zooms in past 1", () => {
    const view = readableViewport({ x: 0, y: 0, width: 300, height: 100 }, pane);
    expect(view.zoom).toBe(1);
    expect(view.x).toBeCloseTo((900 - 300) / 2);
    expect(view.y).toBeCloseTo((600 - 100) / 2);
  });

  it("fits a graph that is a little too wide by zooming out, while boxes stay readable", () => {
    const view = readableViewport({ x: 0, y: 0, width: 900, height: 200 }, pane);
    expect(view.zoom).toBeCloseTo((900 - 64) / 900);
    expect(CARD_WIDTH * view.zoom).toBeGreaterThanOrEqual(READABLE_CARD_PX);
  });

  it("does not shrink below the floor for a long graph: it opens at the start and pans", () => {
    const view = readableViewport({ x: 100, y: 20, width: 5000, height: 200 }, pane);
    expect(view.zoom).toBe(READABLE_ZOOM);
    expect(CARD_WIDTH * view.zoom).toBeGreaterThanOrEqual(READABLE_CARD_PX);
    // The left edge of the graph sits at the padding, so its start is on screen.
    expect(100 * view.zoom + view.x).toBeCloseTo(32);
  });

  it("pins the top edge when the graph is too tall", () => {
    const view = readableViewport({ x: 0, y: 50, width: 300, height: 4000 }, pane);
    expect(view.zoom).toBe(READABLE_ZOOM);
    expect(50 * view.zoom + view.y).toBeCloseTo(32);
  });
});
