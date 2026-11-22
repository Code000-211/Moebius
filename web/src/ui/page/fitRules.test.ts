import { describe, expect, it } from "vitest";
import { deepestLevel, overflows, partsOfLevel } from "./fitRules";

describe("partsOfLevel", () => {
  it("draws everything at level 0", () => {
    expect(partsOfLevel(0, 5)).toEqual({ compactPrimary: false, hiddenTabs: 0 });
  });

  it("drops the primary's label first, before any tab moves", () => {
    expect(partsOfLevel(1, 5)).toEqual({ compactPrimary: true, hiddenTabs: 0 });
  });

  it("then moves one more tab per level", () => {
    expect(partsOfLevel(2, 5)).toEqual({ compactPrimary: true, hiddenTabs: 1 });
    expect(partsOfLevel(4, 5)).toEqual({ compactPrimary: true, hiddenTabs: 3 });
  });

  it("never moves the last tab left, nor any tab of a header without tabs", () => {
    expect(partsOfLevel(99, 5).hiddenTabs).toBe(4);
    expect(partsOfLevel(99, 0).hiddenTabs).toBe(0);
    expect(partsOfLevel(99, 1).hiddenTabs).toBe(0);
  });
});

describe("deepestLevel", () => {
  it("is the label, then every tab but one", () => {
    expect(deepestLevel(5)).toBe(5);
    expect(deepestLevel(1)).toBe(1);
    expect(deepestLevel(0)).toBe(1);
  });
});

describe("overflows", () => {
  it("is false at the padding's edge and within half a pixel, true beyond", () => {
    expect(overflows(900, 900)).toBe(false);
    expect(overflows(900.4, 900)).toBe(false);
    expect(overflows(901, 900)).toBe(true);
  });
});
