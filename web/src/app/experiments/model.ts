/**
 * The logic of the Experiments page, as `screens/experiments/` of the desktop app has it: the tag of a research's
 * status, the numbers and changes of the table, the brief being written (what makes it ready to store) and what a
 * research's status allows a person to do.
 */

import type { ResearchAction } from "../../api/calls/improve";
import type { Better, Brief, Experiment, Research, ResearchStatus } from "../../api/types/improve";
import type { TagTone } from "../../ui/chat/Tag";

/** The tokens a new brief starts with. */
export const DEFAULT_BUDGET = "200000";

export function statusTag(status: ResearchStatus): { label: string; tone: TagTone } {
  switch (status) {
    case "confirmed":
      return { label: "Confirmed", tone: "amber" };
    case "running":
      return { label: "Running", tone: "teal" };
    case "done":
      return { label: "Done", tone: "teal" };
    case "spent":
      return { label: "Spent", tone: "plain" };
    case "stopped":
      return { label: "Stopped", tone: "plain" };
    case "failed":
      return { label: "Failed", tone: "red" };
    default:
      return { label: "Draft", tone: "plain" };
  }
}

/** A number as the page shows it: no more than four decimals, none when it is whole. */
export function number(value: number): string {
  const shown = value.toFixed(4);
  return shown.replace(/0+$/, "").replace(/\.$/, "");
}

/** How far `value` is from `baseline`, in percent of the baseline: `-20 %`; none for a baseline of 0. */
export function change(baseline: number, value: number): string | null {
  if (baseline === 0) return null;
  const percent = ((value - baseline) / Math.abs(baseline)) * 100;
  return `${percent.toFixed(1).replace(/\.0$/, "")} %`;
}

/** What an experiment came to: kept, not kept, or failed when the measurement gave no number. */
export function result(experiment: Experiment): { label: string; tone: TagTone } {
  if (experiment.kept) return { label: "Kept", tone: "teal" };
  return experiment.value === null
    ? { label: "Failed", tone: "red" }
    : { label: "Not kept", tone: "plain" };
}

/** The brief being written, with the repository it is for. */
export interface Form {
  repository: number | null;
  goal: string;
  measure: string;
  better: Better;
  budget: string;
}

export function emptyForm(repository: number | null = null): Form {
  return { repository, goal: "", measure: "", better: "lower", budget: DEFAULT_BUDGET };
}

export function formOf(research: Research): Form {
  return {
    repository: research.repository_id,
    goal: research.goal,
    measure: research.measure,
    better: research.better === "higher" ? "higher" : "lower",
    budget: String(research.token_budget),
  };
}

/** The token budget, if what was typed is a number above 0. */
export function budgetOf(form: Form): number | null {
  const budget = Number(form.budget.trim());
  return form.budget.trim() !== "" && Number.isInteger(budget) && budget > 0 ? budget : null;
}

/** The brief to send, when the form has a repository, a goal, a measure and a budget. */
export function briefOf(form: Form): Brief | null {
  const budget = budgetOf(form);
  if (form.repository === null || budget === null) return null;
  if (form.goal.trim() === "" || form.measure.trim() === "") return null;
  return {
    goal: form.goal.trim(),
    measure: form.measure.trim(),
    better: form.better,
    token_budget: budget,
  };
}

/** The buttons a research's status allows: a draft is confirmed or dropped, a confirmed one started or dropped, a running one stopped. */
export function actions(
  status: ResearchStatus,
): Array<{ label: string; action: ResearchAction; primary: boolean }> {
  switch (status) {
    case "draft":
      return [
        { label: "Confirm", action: "confirm", primary: true },
        { label: "Drop", action: "stop", primary: false },
      ];
    case "confirmed":
      return [
        { label: "Start now", action: "start", primary: true },
        { label: "Drop", action: "stop", primary: false },
      ];
    case "running":
      return [{ label: "Stop", action: "stop", primary: false }];
    default:
      return [];
  }
}
