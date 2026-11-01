/**
 * The logic of the Usage page, apart from React: the windows, how the tiles tell their notes, the colour each model
 * keeps on every chart, the cells of the heatmap of the last year and the bars of where the model time goes
 * (the desktop app's `screens/usage`).
 */
import type { ActivityUsage, UsageReport } from "../../api/types/usage";
import {
  dayNumber,
  duration,
  grouped,
  MONTHS,
  monthAndDay,
  percent,
  share,
  short,
  WEEKDAYS,
  weekday,
} from "./figures";

export interface Window {
  label: string;
  days: number;
}

export const WINDOWS: readonly Window[] = [
  { label: "Today", days: 1 },
  { label: "7 days", days: 7 },
  { label: "30 days", days: 30 },
];

/** How a total over the window is told: "today", "in the last 7 days". */
export function span(days: number): string {
  return days === 1 ? "today" : `in the last ${days} days`;
}

/** The tokens used today and the day's budget, for the meter of the sidebar. */
export function todayOf(report: UsageReport): { tokens: number; budget: number | null } {
  const day = report.days.find((each) => each.date === report.today);
  return { tokens: day?.tokens ?? 0, budget: report.totals.day_budget };
}

export interface Tile {
  label: string;
  value: string;
  note: string;
}

/** The four tiles of the totals. */
export function tiles(report: UsageReport, days: number): Tile[] {
  const totals = report.totals;
  const tokensNote =
    days === 1
      ? totals.day_budget !== null
        ? `${share(totals.tokens, totals.day_budget)} of the ${short(totals.day_budget)} daily budget`
        : "Input and output tokens"
      : `${short(Math.floor(totals.tokens / days))} tokens a day on average`;
  const callsNote =
    totals.calls > 0
      ? `${grouped(Math.floor(totals.tokens / totals.calls))} tokens per call on average`
      : "No model call yet";
  return [
    { label: "Tokens", value: short(totals.tokens), note: tokensNote },
    { label: "Model calls", value: grouped(totals.calls), note: callsNote },
    {
      label: "From cache",
      value: percent(totals.cached_share),
      note: "Input tokens served from cache",
    },
    {
      label: "Model time",
      value: duration(totals.model_ms / 1000),
      note: "Time the models spent answering",
    },
  ];
}

/** The models in the order they appear in the heatmap, then in the table: the order of their colours. */
function modelOrder(report: UsageReport): string[] {
  const order: string[] = [];
  for (const name of [...report.days.map((d) => d.model), ...report.models.map((m) => m.model)]) {
    if (!order.includes(name)) order.push(name);
  }
  return order;
}

/** The colour of each model, as the number (1 to 6) of its CSS variable `--model-<n>`. */
export function modelColours(report: UsageReport): (model: string) => number {
  const order = modelOrder(report);
  return (model) => (Math.max(0, order.indexOf(model)) % 6) + 1;
}

/** The weeks of the heatmap: a year and the week it starts in. */
export const WEEKS = 53;

/** The weights of the four shades of a model's colour against an empty cell. */
export const SHADE_WEIGHT = [32, 54, 78, 100] as const;

export interface Cell {
  /** The day number. */
  day: number;
  date: string;
  /** Null for a day after today, 0 for a day without usage. */
  level: number | null;
  model: string | null;
  tokens: number;
}

export interface Heat {
  /** 7 rows (Sunday first) of `WEEKS` cells. */
  rows: Cell[][];
  /** The name of a month over the weeks that start in it, as far as there is room for it. */
  months: Array<{ name: string; weeks: number }>;
  /** The model that led some day, in colour order, for the legend. */
  leading: string[];
  /** The line that sums the box up. */
  aside: string;
}

function dateOf(day: number): string {
  return new Date(day * 86_400_000).toISOString().slice(0, 10);
}

/** The heatmap of the last 53 weeks: a column a week, a cell a day, in one of four shades by the day's tokens against the busiest. */
export function heat(report: UsageReport): Heat | null {
  const today = dayNumber(report.today);
  if (today === null) return null;
  const first = today - weekday(today) - (WEEKS - 1) * 7;
  const byDay = new Map<number, (typeof report.days)[number]>();
  for (const day of report.days) {
    const number = dayNumber(day.date);
    if (number !== null) byDay.set(number, day);
  }
  const busiest = report.days.reduce<(typeof report.days)[number] | null>(
    (best, day) => (best === null || day.tokens > best.tokens ? day : best),
    null,
  );
  const most = Math.max(1, busiest?.tokens ?? 1);
  const rows: Cell[][] = [];
  for (let row = 0; row < 7; row += 1) {
    const cells: Cell[] = [];
    for (let week = 0; week < WEEKS; week += 1) {
      const day = first + week * 7 + row;
      const used = byDay.get(day);
      let level: number | null = null;
      if (day <= today) {
        level =
          used && used.tokens > 0
            ? Math.min(4, Math.max(1, Math.ceil((used.tokens * 4) / most)))
            : 0;
      }
      cells.push({
        day,
        date: dateOf(day),
        level,
        model: used?.model ?? null,
        tokens: used?.tokens ?? 0,
      });
    }
    rows.push(cells);
  }
  const months: Heat["months"] = [];
  const runs: Array<{ month: number; weeks: number }> = [];
  for (let week = 0; week < WEEKS; week += 1) {
    const [month] = monthAndDay(first + week * 7);
    const last = runs.at(-1);
    if (last && last.month === month) last.weeks += 1;
    else runs.push({ month, weeks: 1 });
  }
  for (const run of runs) {
    months.push({ name: run.weeks >= 2 ? (MONTHS[run.month] ?? "") : "", weeks: run.weeks });
  }
  const leading = modelOrder(report).filter((model) =>
    report.days.some((day) => day.model === model),
  );
  let aside = "No tokens in the last year";
  if (busiest) {
    const total = report.days.reduce((sum, day) => sum + day.tokens, 0);
    const number = dayNumber(busiest.date) ?? today;
    const [month, ofMonth] = monthAndDay(number);
    aside = `${short(total)} tokens on ${report.days.length} days in the last year · busiest day ${WEEKDAYS[weekday(number)]} ${ofMonth} ${MONTHS[month]} with ${short(busiest.tokens)}`;
  }
  return { rows, months, leading, aside };
}

/** The most activities the time box shows before it gathers the rest under "Other". */
export const ACTIVITY_BARS = 8;

/** What the model did with some of its time, as the time box names it. */
export interface Doing {
  name: string;
  /** Whether the name is a tool's, which is set in the code font. */
  tool: boolean;
  calls: number;
  modelMs: number;
}

/** The activities as the bars of the time box: the first eight by time, then the rest as "Other". */
export function doings(activities: readonly ActivityUsage[]): Doing[] {
  const list: Doing[] = activities.map((each) => {
    const [name, tool] =
      each.activity === "answering"
        ? ["Answering", false]
        : each.activity === "unknown"
          ? ["Unknown", false]
          : [each.activity, true];
    return {
      name: name as string,
      tool: tool as boolean,
      calls: each.calls,
      modelMs: each.model_ms,
    };
  });
  if (list.length <= ACTIVITY_BARS) return list;
  const rest = list.slice(ACTIVITY_BARS);
  return [
    ...list.slice(0, ACTIVITY_BARS),
    {
      name: "Other",
      tool: false,
      calls: rest.reduce((sum, each) => sum + each.calls, 0),
      modelMs: rest.reduce((sum, each) => sum + each.modelMs, 0),
    },
  ];
}

/** What a lens check's phase is called. */
export function lensWhen(phase: string): string {
  return phase === "deep"
    ? "Before the run ends"
    : phase === "syntax"
      ? "Syntax, after an edit"
      : "After an edit";
}
