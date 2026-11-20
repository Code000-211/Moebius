/**
 * What the Milestones page works out from the milestones the server answered: their progress in whole percent, their
 * target dates in words, the sorting and the one filter (a project), and the span of each milestone on the timeline, from
 * the day it was made to its target date.
 */
import type { MilestoneView } from "../../api/types/milestones";

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** How far a milestone is, in whole percent; one without cards is at 0. */
export function percent(done: number, cards: number): number {
  return cards === 0 ? 0 : Math.round((done / cards) * 100);
}

/** `done of cards cards`, the words under a progress bar. */
export function progressWords(done: number, cards: number): string {
  return `${done} of ${cards} ${cards === 1 ? "card" : "cards"}`;
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

/** The first and last day of a milestone on the axis: the day it was made to its target date, a day when made late. */
export function spanOf(milestone: MilestoneView): { start: string; end: string } {
  const made = isoOf(milestone.created_at);
  return {
    start: [made, milestone.target_date].sort()[0] ?? made,
    end: milestone.target_date,
  };
}
