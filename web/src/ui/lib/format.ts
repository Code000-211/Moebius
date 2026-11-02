/**
 * Words a page says out loud, in the one way the standard asks for: a count and its noun agreeing, so a single
 * repository is "1 repository" and not "1 repositories", and a recent day named for where it sits rather than as four
 * digits. Every page counts something and lists some days; none of them should write it again.
 */

/**
 * The name a file or a folder is looked for by: the last part of a path, whichever way the server's system points.
 * The whole path stays available as a tooltip, because the folder a file is in is asked about far less than it is named.
 */
export function fileName(path: string): string {
  const parts = path.split(/[/\\]/).filter((part) => part !== "");
  return parts.at(-1) ?? path;
}

/** A count with its noun: `plural(1, "run", "runs")` is "1 run", `plural(3, "run", "runs")` is "3 runs". */
export function plural(n: number, one: string, many: string): string {
  return `${n} ${noun(n, one, many)}`;
}

/**
 * The noun alone, agreeing with the count, for a number already written out — a count grouped by thousands is
 * `grouped(1240)` plus `noun(1240, "call", "calls")`, and putting the whole phrase through `plural` would say the
 * digits twice.
 */
export function noun(n: number, one: string, many: string): string {
  return n === 1 ? one : many;
}

/**
 * How long something took, the one way the app says a duration: `5s`, then minutes, then hours. A step that has
 * finished is labelled with one — `Thought 12s` — so the line says what happened and for how long rather than only
 * what happened. `ago` answers the other question, when something was.
 */
export function elapsed(seconds: number): string {
  const whole = Math.max(0, Math.floor(seconds));
  if (whole < 60) return `${whole}s`;
  if (whole < 3_600) return `${Math.floor(whole / 60)}m`;
  return `${Math.floor(whole / 3_600)}h`;
}

/** A day in milliseconds, so two dates compare as whole days rather than as clocks. */
const DAY = 86_400_000;

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** The whole days since the epoch that a `YYYY-MM-DD` names, or null when it names nothing. */
function dayIndex(date: string): number | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(date);
  if (!match) return null;
  const [year, month, day] = [Number(match[1]), Number(match[2]), Number(match[3])];
  return Math.round(Date.UTC(year, month - 1, day) / DAY);
}

/** Today, as the `YYYY-MM-DD` the server writes, by the browser's calendar. */
export function today(): string {
  const now = new Date();
  const month = `${now.getMonth() + 1}`.padStart(2, "0");
  const day = `${now.getDate()}`.padStart(2, "0");
  return `${now.getFullYear()}-${month}-${day}`;
}

/**
 * How long ago `then` was, the one way the app says a gap: "just now", then minutes, hours, days. `now` and `then`
 * are Unix seconds, and every list that stamps a row passes the same `now` it stamps its other rows with, so a page
 * does not call one row "just now" and its neighbour "1 min ago" for want of a shared clock.
 */
export function ago(now: number, then: number): string {
  const seconds = Math.max(0, now - then);
  if (seconds < 60) return "just now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)} min ago`;
  if (seconds < 86_400) return `${Math.floor(seconds / 3600)} h ago`;
  return `${Math.floor(seconds / 86_400)} d ago`;
}

/**
 * A day in words: "Today", then "Yesterday", then the date — `27 Oct`, with the year once it leaves this one. The two
 * days nearest now are the two nobody has to work out; from two days back a date is shorter and steadier than "3 days
 * ago", which goes stale while the page stays open.
 *
 * `now` is the day the page counts from; one that already knows what day the server thinks it is passes that, so the
 * two agree and a page left open past midnight does not move its own days.
 */
export function dayName(date: string, now = today()): string {
  const wanted = dayIndex(date);
  const current = dayIndex(now);
  if (wanted === null || current === null) return date;
  const back = current - wanted;
  if (back === 0) return "Today";
  if (back === 1) return "Yesterday";
  const parts = new Date(wanted * DAY);
  const thisYear = new Date(current * DAY).getUTCFullYear();
  const month = MONTHS[parts.getUTCMonth()] ?? "";
  const dayOfMonth = parts.getUTCDate();
  return parts.getUTCFullYear() === thisYear
    ? `${dayOfMonth} ${month}`
    : `${dayOfMonth} ${month} ${parts.getUTCFullYear()}`;
}
