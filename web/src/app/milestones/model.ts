/**
 * What the Milestones page works out from the milestones the server answered: their progress in whole percent, their
 * target dates in words, the sorting and the one filter (a project), and the timeline — each milestone a bar from the
 * day it was made to its target date, on an axis of months, with a line at today.
 */
import type { MilestoneView } from "../../api/types/milestones";

const DAY = 86_400_000;
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** How far a milestone is, in whole percent; one without cards is at 0. */
export function percent(done: number, cards: number): number {
  return cards === 0 ? 0 : Math.round((done / cards) * 100);
}

/** `done of cards cards`, the words under a progress bar. */
export function progressWords(done: number, cards: number): string {
  return `${done} of ${cards} ${cards === 1 ? "card" : "cards"}`;
}

/** The milliseconds of midnight UTC of a `YYYY-MM-DD` date. */
function dayOf(iso: string): number {
  const [year, month, day] = iso.split("-").map(Number);
  return Date.UTC(year ?? 1970, (month ?? 1) - 1, day ?? 1);
}

/** The date as `YYYY-MM-DD` of a time in seconds, in UTC. */
export function isoOf(seconds: number): string {
  return new Date(seconds * 1000).toISOString().slice(0, 10);
}

/** A target date in words: `Dec 12`, and the year when it is not the one of `today`. */
export function dateWords(iso: string, today: string): string {
  const [year, month, day] = iso.split("-").map(Number);
  const text = `${MONTHS[(month ?? 1) - 1]} ${day}`;
  return year === Number(today.slice(0, 4)) ? text : `${text}, ${year}`;
}

/** Whether the target date has passed with cards still open (or none counted). */
export function overdue(milestone: MilestoneView, today: string): boolean {
  return milestone.target_date < today && milestone.done < milestone.cards;
}

/** How a milestone's projects read: one name, or a count. */
export function projectsWords(milestone: MilestoneView): string {
  const { projects } = milestone;
  if (projects.length === 0) return "No cards yet";
  if (projects.length === 1) return projects[0]?.name ?? "";
  return `${projects.length} projects`;
}

/** The sorts Display offers, each as `[value, label]`; the first is the default. */
export const SORTS: Array<[string, string]> = [
  ["", "Target date"],
  ["name", "Name"],
  ["progress", "Progress"],
];

/** The milestones in the order `sort` names; the target date is the server's order and the tie-break of the others. */
export function sorted(list: MilestoneView[], sort: string): MilestoneView[] {
  const byDate = (a: MilestoneView, b: MilestoneView) =>
    a.target_date.localeCompare(b.target_date) || a.id - b.id;
  const order: Record<string, (a: MilestoneView, b: MilestoneView) => number> = {
    name: (a, b) => a.name.localeCompare(b.name) || byDate(a, b),
    progress: (a, b) => percent(b.done, b.cards) - percent(a.done, a.cards) || byDate(a, b),
  };
  return [...list].sort(order[sort] ?? byDate);
}

/** The milestones that hold cards of the project `project` (its id as text), or all of them when it is empty. */
export function inProject(list: MilestoneView[], project: string): MilestoneView[] {
  if (project === "") return list;
  return list.filter((m) => m.projects.some((p) => String(p.id) === project));
}

/** One month of the axis: its name and where it starts, in percent of the axis. */
export interface Month {
  label: string;
  left: number;
}

/** One bar of the timeline, in percent of the axis; `progress` is the part of the bar that is done. */
export interface Bar {
  milestone: MilestoneView;
  left: number;
  width: number;
  progress: number;
}

export interface Timeline {
  months: Month[];
  bars: Bar[];
  /** Where today is, in percent of the axis. */
  today: number;
}

/** The narrowest a bar is, in percent of the axis, so a milestone made on its target date can still be seen. */
const LEAST = 1.5;

/** The timeline of `list`, on an axis of whole months that holds every bar and `today`. */
export function timeline(list: MilestoneView[], today: string): Timeline {
  const todayDay = dayOf(today);
  const spans = list.map((m) => {
    const end = dayOf(m.target_date);
    return { m, start: Math.min(dayOf(isoOf(m.created_at)), end), end };
  });
  const first = new Date(Math.min(todayDay, ...spans.map((s) => s.start)));
  const last = new Date(Math.max(todayDay, ...spans.map((s) => s.end)));
  const from = Date.UTC(first.getUTCFullYear(), first.getUTCMonth(), 1);
  const to = Date.UTC(last.getUTCFullYear(), last.getUTCMonth() + 1, 1);
  const span = to - from;
  const at = (day: number) => ((day - from) / span) * 100;
  const months: Month[] = [];
  for (let t = from; t < to; ) {
    const d = new Date(t);
    const january = d.getUTCMonth() === 0 || t === from;
    months.push({
      label: january
        ? `${MONTHS[d.getUTCMonth()]} ${d.getUTCFullYear()}`
        : (MONTHS[d.getUTCMonth()] ?? ""),
      left: at(t),
    });
    t = Date.UTC(d.getUTCFullYear(), d.getUTCMonth() + 1, 1);
  }
  const bars = spans.map(({ m, start, end }) => ({
    milestone: m,
    left: at(start),
    width: Math.max(((end - start + DAY) / span) * 100, LEAST),
    progress: percent(m.done, m.cards),
  }));
  return { months, bars, today: at(todayDay) };
}
