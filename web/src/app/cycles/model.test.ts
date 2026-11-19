import { describe, expect, it } from "vitest";
import type { CycleView } from "../../api/types/cycles";
import { chart, nextCycle, ordered, percent, periodWords, progressWords, stateOf } from "./model";

const cycle = (over: Partial<CycleView>): CycleView => ({
  id: 1,
  project_id: 1,
  name: "One",
  starts_on: "2026-11-02",
  ends_on: "2026-11-15",
  closed_at: null,
  created_at: 0,
  cards: 0,
  done: 0,
  points: 0,
  points_done: 0,
  ...over,
});

describe("progress", () => {
  it("is counted in points when the cards have them, else in cards", () => {
    expect(percent(cycle({ points: 8, points_done: 2, cards: 3, done: 3 }))).toBe(25);
    expect(percent(cycle({ cards: 4, done: 1 }))).toBe(25);
    expect(percent(cycle({}))).toBe(0);
  });

  it("says both in words", () => {
    expect(progressWords(cycle({ points: 8, points_done: 3, cards: 3, done: 1 }))).toBe(
      "3 of 8 points · 1 of 3 cards",
    );
    expect(progressWords(cycle({ cards: 1 }))).toBe("0 of 1 card");
  });
});

describe("dates", () => {
  it("are written without the year of today", () => {
    expect(periodWords(cycle({}), "2026-11-19")).toBe("Nov 2 – Nov 15");
    expect(periodWords(cycle({ ends_on: "2027-01-03" }), "2026-11-19")).toBe("Nov 2 – Jan 3, 2027");
  });

  it("tell where a cycle stands", () => {
    expect(stateOf(cycle({}), "2026-11-01")).toBe("upcoming");
    expect(stateOf(cycle({}), "2026-11-15")).toBe("running");
    expect(stateOf(cycle({}), "2026-11-16")).toBe("overdue");
    expect(stateOf(cycle({ closed_at: 5 }), "2026-11-16")).toBe("closed");
  });
});

describe("the list", () => {
  const list = [
    cycle({ id: 1, name: "One", starts_on: "2026-11-02" }),
    cycle({ id: 2, name: "Two", starts_on: "2026-11-16" }),
    cycle({ id: 3, name: "Old", starts_on: "2026-10-01", closed_at: 9 }),
    cycle({ id: 4, name: "Three", starts_on: "2026-11-30" }),
  ];

  it("puts the open cycles first, the latest start first", () => {
    expect(ordered(list).map((c) => c.name)).toEqual(["Three", "Two", "One", "Old"]);
  });

  it("carries to the open cycle that starts next, never a closed or an earlier one", () => {
    expect(nextCycle(list, list[0] as CycleView)?.name).toBe("Two");
    expect(nextCycle(list, list[3] as CycleView)).toBeUndefined();
    expect(nextCycle(list, list[2] as CycleView)?.name).toBe("One");
  });
});

describe("the burn-up chart", () => {
  it("scales both lines to the largest scope", () => {
    const c = chart(
      [
        { day: "2026-11-02", scope: 4, done: 0 },
        { day: "2026-11-03", scope: 8, done: 2 },
        { day: "2026-11-04", scope: 8, done: 8 },
      ],
      100,
      40,
    );
    expect(c.top).toBe(8);
    expect(c.scope).toBe("0.0,20.0 50.0,0.0 100.0,0.0");
    expect(c.done).toBe("0.0,40.0 50.0,30.0 100.0,0.0");
  });

  it("has no line for no day and a point for one day", () => {
    expect(chart([], 100, 40).scope).toBe("");
    expect(chart([{ day: "2026-11-02", scope: 0, done: 0 }], 100, 40).days).toBe(1);
  });
});
