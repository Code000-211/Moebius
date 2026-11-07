import { describe, expect, it } from "vitest";
import type { UsageReport } from "../../api/types/usage";
import { dayNumber } from "./figures";
import {
  dayMeter,
  doings,
  heat,
  modelColours,
  span,
  tiles,
  todayOf,
  tokenFigure,
  WEEKS,
} from "./model";

const report = (extra: Partial<UsageReport> = {}): UsageReport => ({
  today: "2026-10-28",
  days_in_window: 1,
  totals: {
    tokens: 1_840_000,
    calls: 1284,
    cached_share: 0.61,
    model_ms: 11_520_000,
    day_budget: 8_000_000,
  },
  days: [
    { date: "2026-10-26", tokens: 1_000_000, model: "sonnet" },
    { date: "2026-10-27", tokens: 250_000, model: "qwen" },
    { date: "2026-10-28", tokens: 500_000, model: "sonnet" },
  ],
  models: [
    { model: "qwen", tokens: 960_000, calls: 690, input_tokens: 700_000, cached_tokens: 520_000 },
    { model: "haiku", tokens: 100, calls: 1, input_tokens: 1, cached_tokens: 0 },
  ],
  agents: [],
  activities: [],
  tool_waits: [],
  ...extra,
});

describe("the tiles", () => {
  it("carry the totals the focal figure does not, each with its own note", () => {
    const today = tiles(report());
    expect(today.map((tile) => [tile.label, tile.value])).toEqual([
      ["Model calls", "1,284"],
      ["From cache", "61%"],
      ["Model time", "3 h 12 m"],
    ]);
    expect(today[0]?.note).toBe("1,433 tokens per call on average");
    const free = report();
    free.totals.calls = 0;
    expect(tiles(free)[0]?.note).toBe("No model call yet");
  });

  it("name the focal token figure for the window it counts", () => {
    const day = tokenFigure(report());
    expect([day.title, day.value, day.note]).toEqual([
      "Tokens today",
      "500k",
      "of the 8M token budget, 6% used",
    ]);
    expect(day.meter?.word).toBe("Within budget");
    // A longer window counts what it covers and leaves the day's meter behind.
    const week = report({ days_in_window: 7 });
    week.totals.tokens = 12_900_000;
    const seven = tokenFigure(week);
    expect([seven.title, seven.value, seven.note]).toEqual([
      "Tokens, last 7 days",
      "12.9M",
      "1.84M a day on average, against a daily budget of 8M",
    ]);
    expect(seven.meter).toBeNull();
    const free = report({ days_in_window: 7 });
    free.totals.day_budget = null;
    expect(tokenFigure(free).note).toBe("263k a day on average");
    const noBudget = report();
    noBudget.totals.day_budget = null;
    expect(tokenFigure(noBudget)).toMatchObject({
      title: "Tokens today",
      note: "Input and output tokens",
      meter: null,
    });
  });

  it("tell the span of a window and the tokens used today", () => {
    expect(span(1)).toBe("today");
    expect(span(7)).toBe("in the last 7 days");
    expect(todayOf(report())).toEqual({ tokens: 500_000, budget: 8_000_000 });
    expect(todayOf(report({ days: [] }))).toEqual({ tokens: 0, budget: 8_000_000 });
  });
});

describe("the colours of the models", () => {
  it("follow the order of appearance in the heatmap, then the table, and wrap after six", () => {
    const colour = modelColours(report());
    expect(["sonnet", "qwen", "haiku"].map(colour)).toEqual([1, 2, 3]);
    expect(colour("unknown")).toBe(1);
    const many = report({
      days: Array.from({ length: 7 }, (_, at) => ({
        date: `2026-10-0${at + 1}`,
        tokens: 1,
        model: `m${at}`,
      })),
    });
    expect(modelColours(many)("m6")).toBe(1);
  });
});

describe("today against the day's budget", () => {
  /** Today's tokens, with the rest of the year left as it is. */
  const todayUses = (tokens: number) => {
    const each = report();
    each.days = each.days.map((day) => (day.date === each.today ? { ...day, tokens } : day));
    return each;
  };

  it("says how today stands in words as well as a hue", () => {
    expect(dayMeter(todayUses(500_000))).toMatchObject({
      tokens: 500_000,
      budget: 8_000_000,
      used: 0.0625,
      word: "Within budget",
      tone: "progress",
    });
    expect(dayMeter(todayUses(6_600_000))).toMatchObject({
      word: "Near the budget",
      tone: "attention",
    });
    expect(dayMeter(todayUses(8_000_000))).toMatchObject({
      word: "Over budget",
      tone: "error",
    });
  });

  it("has nothing to say when there is no budget", () => {
    const free = report();
    free.totals.day_budget = null;
    expect(dayMeter(free)).toBeNull();
    const zero = report();
    zero.totals.day_budget = 0;
    expect(dayMeter(zero)).toBeNull();
  });
});

describe("the heatmap", () => {
  it("has a cell for each day of 53 weeks, Sunday on top, the last one today, none after it", () => {
    const map = heat(report());
    expect(map).not.toBeNull();
    const rows = map?.rows ?? [];
    expect(rows).toHaveLength(7);
    expect(rows.every((row) => row.length === WEEKS)).toBe(true);
    const today = dayNumber("2026-10-28") as number;
    // 2026-10-28 is a Wednesday: the last column has Sunday to Wednesday, then nothing.
    const last = rows.map((row) => row.at(-1));
    expect(last.map((cell) => cell?.level === null)).toEqual([
      false,
      false,
      false,
      false,
      true,
      true,
      true,
    ]);
    expect(last[3]?.day).toBe(today);
    expect(last[3]?.date).toBe("2026-10-28");
  });

  it("shades a day by its tokens against the busiest day, in four levels", () => {
    const map = heat(report());
    const find = (date: string) => map?.rows.flat().find((cell) => cell.date === date);
    expect(find("2026-10-26")).toMatchObject({ level: 4, model: "sonnet", tokens: 1_000_000 });
    expect(find("2026-10-28")?.level).toBe(2);
    expect(find("2026-10-27")?.level).toBe(1);
    expect(find("2026-10-25")).toMatchObject({ level: 0, model: null });
  });

  it("names the models that led a day, and sums the year up", () => {
    const map = heat(report());
    expect(map?.leading).toEqual(["sonnet", "qwen"]);
    expect(map?.aside).toBe(
      "1.75M tokens on 3 days in the last year · busiest day Mon 26 Oct with 1M",
    );
    expect(heat(report({ days: [] }))?.aside).toBe("No tokens in the last year");
    expect(heat(report({ today: "soon" }))).toBeNull();
  });

  it("names a month over the weeks that start in it, as far as there is room", () => {
    const months = heat(report())?.months ?? [];
    expect(months.reduce((sum, month) => sum + month.weeks, 0)).toBe(WEEKS);
    expect(months.some((month) => month.name === "Oct")).toBe(true);
  });
});

describe("where the model time goes", () => {
  const activity = (name: string, ms: number, calls: number) => ({
    activity: name,
    model_ms: ms,
    calls,
    output_tokens: 0,
  });

  it("names the activities and sets tools in the code font", () => {
    expect(
      doings([
        activity("bash", 9000, 214),
        activity("answering", 5000, 40),
        activity("unknown", 1000, 3),
      ]).map((each) => [each.name, each.tool, each.calls]),
    ).toEqual([
      ["bash", true, 214],
      ["Answering", false, 40],
      ["Unknown", false, 3],
    ]);
  });

  it("shows eight activities and gathers the rest as other", () => {
    const list = doings(Array.from({ length: 11 }, (_, at) => activity(`tool${at}`, 100 - at, 1)));
    expect(list).toHaveLength(9);
    expect(list[7]?.name).toBe("tool7");
    expect(list[8]).toEqual({ name: "Other", tool: false, calls: 3, modelMs: 92 + 91 + 90 });
  });
});
