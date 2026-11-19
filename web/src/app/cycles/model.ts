/**
 * What the Cycles page works out from what the server answered: progress in whole percent, the dates of a cycle in
 * words, which cycle a closing carries cards to by default, and the burn-up as the points of two lines in a chart.
 */
import type { BurnUpDay, Cycle, CycleView } from "../../api/types/cycles";

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** How far a cycle is, in whole percent of its points when it has any, else of its cards. */
export function percent(cycle: CycleView): number {
  if (cycle.points > 0) return Math.round((cycle.points_done / cycle.points) * 100);
  return cycle.cards === 0 ? 0 : Math.round((cycle.done / cycle.cards) * 100);
}

/** `3 of 8 points · 1 of 3 cards`, the words under a progress bar. */
export function progressWords(cycle: CycleView): string {
  const cards = `${cycle.done} of ${cycle.cards} ${cycle.cards === 1 ? "card" : "cards"}`;
  return cycle.points > 0 ? `${cycle.points_done} of ${cycle.points} points · ${cards}` : cards;
}

/** `Nov 2 – Nov 15`, with the year of each end that is not the year of `today`. */
export function periodWords(cycle: Cycle, today: string): string {
  const one = (iso: string) => {
    const [year, month, day] = iso.split("-").map(Number);
    const text = `${MONTHS[(month ?? 1) - 1]} ${day}`;
    return year === Number(today.slice(0, 4)) ? text : `${text}, ${year}`;
  };
  return `${one(cycle.starts_on)} – ${one(cycle.ends_on)}`;
}

/** Where a cycle stands on `today`: closed, not started, running or past its end with the cycle still open. */
export function stateOf(
  cycle: Cycle,
  today: string,
): "closed" | "upcoming" | "running" | "overdue" {
  if (cycle.closed_at !== null) return "closed";
  if (today < cycle.starts_on) return "upcoming";
  return today > cycle.ends_on ? "overdue" : "running";
}

/** The cycle a closing carries the cards to when none is chosen: the open one of the project starting next. */
export function nextCycle(list: CycleView[], cycle: Cycle): CycleView | undefined {
  return list
    .filter(
      (other) =>
        other.id !== cycle.id &&
        other.closed_at === null &&
        (other.starts_on > cycle.starts_on ||
          (other.starts_on === cycle.starts_on && other.id > cycle.id)),
    )
    .sort((a, b) => a.starts_on.localeCompare(b.starts_on) || a.id - b.id)[0];
}

/** The open cycles first (the latest start first within each), then the closed ones. */
export function ordered(list: CycleView[]): CycleView[] {
  const rank = (cycle: CycleView) => (cycle.closed_at === null ? 0 : 1);
  return [...list].sort(
    (a, b) => rank(a) - rank(b) || b.starts_on.localeCompare(a.starts_on) || b.id - a.id,
  );
}

/** A burn-up as drawn: the points of two lines in a box of `width` by `height`, and the top of the scale. */
export interface Chart {
  top: number;
  scope: string;
  done: string;
  days: number;
}

/** The points of the two lines of a burn-up (`x,y x,y …`), the top of the scale being the largest scope. */
export function chart(burn: BurnUpDay[], width: number, height: number): Chart {
  const top = Math.max(1, ...burn.map((d) => d.scope));
  const x = (i: number) => (burn.length <= 1 ? 0 : (i / (burn.length - 1)) * width);
  const y = (value: number) => height - (value / top) * height;
  const line = (pick: (d: BurnUpDay) => number) =>
    burn.map((d, i) => `${x(i).toFixed(1)},${y(pick(d)).toFixed(1)}`).join(" ");
  return { top, scope: line((d) => d.scope), done: line((d) => d.done), days: burn.length };
}

/** `1 200 tokens per point`, or a dash while no point is done. */
export function perPointWords(tokens: number | null): string {
  return tokens === null ? "–" : `${tokens.toLocaleString("en")} per point`;
}
