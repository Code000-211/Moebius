import { describe, expect, it } from "vitest";
import type { UsageReport } from "../../api/types/usage";
import { dayNumber } from "./figures";
import { doings, heat, lensWhen, modelColours, span, tiles, todayOf, WEEKS } from "./model";

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
  it("note the day's budget today and the average over a longer window", () => {
    const today = tiles(report(), 1);
    expect(today.map((tile) => [tile.label, tile.value])).toEqual([
      ["Tokens", "1.84M"],
      ["Model calls", "1,284"],
      ["From cache", "61%"],
      ["Model time", "3 h 12 m"],
    ]);
    expect(today[0]?.note).toBe("23% of the 8M daily budget");
    expect(today[1]?.note).toBe("1,433 tokens per call on average");
    expect(tiles(report(), 7)[0]?.note).toBe("263k tokens a day on average");
    const free = report();
    free.totals.day_budget = null;
    free.totals.calls = 0;
    expect(tiles(free, 1)[0]?.note).toBe("Input and output tokens");
    expect(tiles(free, 1)[1]?.note).toBe("No model call yet");
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

  it("names the phases of the lens", () => {
    expect(lensWhen("deep")).toBe("Before the run ends");
    expect(lensWhen("syntax")).toBe("Syntax, after an edit");
    expect(lensWhen("lint")).toBe("After an edit");
  });
});
