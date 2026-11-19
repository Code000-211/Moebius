import { describe, expect, it } from "vitest";
import type { BurnUp } from "../../api/types/analytics";
import { bars, burnWords, columnBars, dayWords, lengthOf, perPoint, widthOf } from "./model";

describe("lengthOf", () => {
  it("names the two largest units", () => {
    expect(lengthOf(45)).toBe("45 s");
    expect(lengthOf(38 * 60 + 5)).toBe("38 m");
    expect(lengthOf(5 * 3600 + 12 * 60)).toBe("5 h 12 m");
    expect(lengthOf(5 * 3600)).toBe("5 h");
    expect(lengthOf(3 * 86_400 + 4 * 3600 + 59)).toBe("3 d 4 h");
    expect(lengthOf(2 * 86_400)).toBe("2 d");
    expect(lengthOf(-5)).toBe("0 s");
  });
});

describe("the bars", () => {
  it("sizes each against the largest and gives anything above nothing a sliver", () => {
    expect(widthOf(0, 10)).toBe(0);
    expect(widthOf(1, 1000)).toBe(2);
    expect(widthOf(5, 10)).toBe(50);
    expect(widthOf(5, 0)).toBe(0);
    const days = [
      { day: "2026-11-17", done: 0, points: 0 },
      { day: "2026-11-18", done: 4, points: 9 },
      { day: "2026-11-19", done: 2, points: 1 },
    ];
    expect(bars(days).map((d) => d.height)).toEqual([0, 100, 50]);
    expect(bars([]).length).toBe(0);
    const columns = [
      { column: "Backlog", cards: 2, total_seconds: 200, mean_seconds: 100 },
      { column: "Ready", cards: 1, total_seconds: 25, mean_seconds: 25 },
    ];
    expect(columnBars(columns).map((c) => c.width)).toEqual([100, 25]);
  });
});

describe("the words", () => {
  it("tell tokens per point, a day and a burn-up", () => {
    expect(perPoint(null)).toBe("–");
    expect(perPoint(12_345)).toBe("12,345");
    expect(dayWords("2026-11-09")).toBe("Nov 9");
    const burn: BurnUp = {
      id: 1,
      name: "One",
      project: null,
      starts_on: "2026-11-02",
      ends_on: "2026-11-15",
      cards: 3,
      done: 2,
      points: 8,
      points_done: 5,
      days: [],
    };
    expect(burnWords(burn)).toBe("5 of 8 points · 2 of 3 cards");
    expect(burnWords({ ...burn, points: 0, cards: 1, done: 0 })).toBe("0 of 1 card");
  });
});
