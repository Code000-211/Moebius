/**
 * What the Analytics page works out from what the server answered: the windows, a length of time in words, the bars of
 * the throughput and of the time in each column, and how a burn-up is told.
 */
import type { Analytics, BurnUp, ColumnTime, ThroughputDay } from "../../api/types/analytics";

export const WINDOWS = [
  { label: "7 days", days: 7 },
  { label: "30 days", days: 30 },
  { label: "90 days", days: 90 },
] as const;

/** The window the page opens with. */
export const DEFAULT_DAYS = 30;

/** A length of time in its two largest units: "3 d 4 h", "5 h 12 m", "38 m", "45 s". */
export function lengthOf(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  const days = Math.floor(total / 86_400);
  const hours = Math.floor((total % 86_400) / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  if (days > 0) return hours > 0 ? `${days} d ${hours} h` : `${days} d`;
  if (hours > 0) return minutes > 0 ? `${hours} h ${minutes} m` : `${hours} h`;
  if (minutes > 0) return `${minutes} m`;
  return `${total} s`;
}

/** The tokens per point as words, or a dash while no point is done. */
export function perPoint(tokens: number | null): string {
  return tokens === null ? "–" : tokens.toLocaleString("en");
}

/** A bar's width in percent against the largest value, with a sliver for anything above nothing. */
export function widthOf(value: number, largest: number): number {
  if (value <= 0 || largest <= 0) return 0;
  return Math.max(2, Math.round((value * 100) / largest));
}

/** The throughput bars: each day's height in percent against the busiest day. */
export function bars(
  days: ThroughputDay[],
): { day: string; done: number; points: number; height: number }[] {
  const busiest = Math.max(0, ...days.map((d) => d.done));
  return days.map((d) => ({ ...d, height: widthOf(d.done, busiest) }));
}

/** The columns with the width of each bar against the column cards wait in longest. */
export function columnBars(columns: ColumnTime[]): (ColumnTime & { width: number })[] {
  const longest = Math.max(0, ...columns.map((c) => c.mean_seconds));
  return columns.map((c) => ({ ...c, width: widthOf(c.mean_seconds, longest) }));
}

/** A day as "Nov 19", with no year. */
export function dayWords(day: string): string {
  const months = [
    "Jan",
    "Feb",
    "Mar",
    "Apr",
    "May",
    "Jun",
    "Jul",
    "Aug",
    "Sep",
    "Oct",
    "Nov",
    "Dec",
  ];
  const [, month, date] = day.split("-").map(Number);
  return `${months[(month ?? 1) - 1]} ${date}`;
}

/** "5 of 8 points done · 2 of 3 cards", the words under a burn-up. */
export function burnWords(burn: BurnUp): string {
  const cards = `${burn.done} of ${burn.cards} ${burn.cards === 1 ? "card" : "cards"}`;
  return burn.points > 0 ? `${burn.points_done} of ${burn.points} points · ${cards}` : cards;
}

/** Whether the answer has nothing to show: no card done, none open and no cycle or milestone. */
export function isEmpty(data: Analytics): boolean {
  return (
    data.totals.done === 0 &&
    data.totals.open === 0 &&
    data.cycles.length === 0 &&
    data.milestones.length === 0 &&
    data.columns.length === 0
  );
}
