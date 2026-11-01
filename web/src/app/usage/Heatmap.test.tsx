import { describe, expect, it } from "vitest";
import { shade } from "./Heatmap";
import { SHADE_WEIGHT } from "./model";

describe("the ramp of the heatmap", () => {
  it("mixes a category colour with the ground of an empty cell, so both themes colour it themselves", () => {
    expect(shade(1, 1)).toBe(`color-mix(in srgb, var(--cat-1) 45%, var(--heat-empty))`);
    expect(shade(3, 4)).toBe(`color-mix(in srgb, var(--cat-3) 100%, var(--heat-empty))`);
  });

  it("has four shades that grow, the faintest one clear of the empty cell", () => {
    expect(SHADE_WEIGHT).toHaveLength(4);
    expect(shade(2, 1)).not.toBe(shade(2, 2));
    expect([...SHADE_WEIGHT].sort((a, b) => a - b)).toEqual([...SHADE_WEIGHT]);
    expect(SHADE_WEIGHT[0]).toBeGreaterThanOrEqual(40);
  });

  it("keeps a level outside the four inside them", () => {
    expect(shade(1, 0)).toBe(shade(1, 1));
    expect(shade(1, 9)).toBe(shade(1, 4));
  });
});
