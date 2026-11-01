/**
 * How the Usage page tells its figures: numbers of tokens in three figures, counts with separators, shares and times,
 * and the dates of the heatmap as day numbers with their weekday and month (the desktop app's `usage/figures.rs`).
 */

export const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"] as const;
export const MONTHS = [
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
] as const;

/** A number in three figures: 960k, 1.84M, 148M. */
export function short(n: number): string {
  if (n < 1000) return String(n);
  const [value, unit] = n < 1_000_000 ? [n / 1e3, "k"] : [n / 1e6, "M"];
  const decimals = (value as number) < 10 ? 2 : (value as number) < 100 ? 1 : 0;
  let shown = (value as number).toFixed(decimals);
  if (shown.includes(".")) shown = shown.replace(/0+$/, "").replace(/\.$/, "");
  return `${shown}${unit}`;
}

/** A number of bytes in three figures: 640 B, 12.4 KB, 1.2 MB. */
export function bytes(n: number): string {
  if (n < 1000) return `${n} B`;
  if (n < 1_000_000) return `${short(n).replace(/k$/, "")} KB`;
  return `${short(n).replace(/M$/, "")} MB`;
}

/** A whole number with thousands separators: 1,284. */
export function grouped(n: number): string {
  const digits = String(Math.abs(Math.trunc(n)));
  const out = digits.replace(/\B(?=(\d{3})+$)/g, ",");
  return n < 0 ? `-${out}` : out;
}

/** A share between 0 and 1 as a percentage. */
export function percent(share: number): string {
  return `${Math.round(share * 100)}%`;
}

/** A part of a whole as a percentage; 0 of nothing. */
export function share(part: number, whole: number): string {
  return percent(whole > 0 ? part / whole : 0);
}

/** A time as hours and minutes ("3 h 12 m"), minutes ("38 m") or seconds ("45 s"). */
export function duration(seconds: number): string {
  const total = Math.floor(seconds);
  if (total < 60) return `${total} s`;
  if (total < 3600) return `${Math.floor(total / 60)} m`;
  return `${Math.floor(total / 3600)} h ${Math.floor((total % 3600) / 60)} m`;
}

/** A time in milliseconds up to a second, then as `duration` says it. */
export function milliseconds(ms: number): string {
  return ms < 1000 ? `${ms} ms` : duration(ms / 1000);
}

/** The day number (days since 1970-01-01) of a `YYYY-MM-DD` date; null for anything else. */
export function dayNumber(date: string): number | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(date);
  if (!match) return null;
  const [year, month, day] = [Number(match[1]), Number(match[2]), Number(match[3])];
  return Math.floor(Date.UTC(year, month - 1, day) / 86_400_000);
}

/** The month (0 to 11) and day of the month of a day number. */
export function monthAndDay(number: number): [number, number] {
  const date = new Date(number * 86_400_000);
  return [date.getUTCMonth(), date.getUTCDate()];
}

/** The day of the week of a day number, Sunday 0 (1970-01-01 was a Thursday). */
export function weekday(number: number): number {
  return (((number + 4) % 7) + 7) % 7;
}
