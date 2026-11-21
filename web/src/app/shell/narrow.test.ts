import { describe, expect, it } from "vitest";
import { isNarrow, NARROW_BELOW } from "./narrow";

describe("the width rule", () => {
  it("is narrow at 999 and wide at 1000", () => {
    expect(NARROW_BELOW).toBe(1000);
    expect(isNarrow(999)).toBe(true);
    expect(isNarrow(1000)).toBe(false);
  });

  it("is narrow at 900 and 800 and wide at 1280", () => {
    expect(isNarrow(900)).toBe(true);
    expect(isNarrow(800)).toBe(true);
    expect(isNarrow(1280)).toBe(false);
  });
});
