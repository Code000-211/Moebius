/**
 * The logic of the Experiments page, as `screens/experiments/` of the former desktop app has it: the word for a research's
 * status, the numbers and changes of the table, the brief being written (what makes it ready to store) and what a
 * research's status allows a person to do.
 */

import type { ResearchAction } from "../../api/calls/improve";
import type {
  Better,
  Brief,
  Budget,
  BudgetKind,
  Experiment,
  Research,
  ResearchStatus,
} from "../../api/types/improve";
import type { TagTone } from "../../ui/chat/Tag";
import { short } from "../usage/figures";

/** The tokens a new brief starts with. */
export const DEFAULT_BUDGET = "200000";

/** The experiments a brief starts with once its budget is counted in experiments. */
export const DEFAULT_EXPERIMENTS = "10";

/** The amount a new budget of this kind starts with. */
export function defaultAmount(kind: BudgetKind): string {
  return kind === "experiments" ? DEFAULT_EXPERIMENTS : DEFAULT_BUDGET;
}

/**
 * The word for a research's status and the hue that echoes it. A hue is only for what it means: a brief that waits
 * for a decision, work under way, work that came to an end, work that stopped for a reason.
 */
export function statusTag(status: ResearchStatus): { label: string; tone: TagTone } {
  switch (status) {
    case "confirmed":
      return { label: "Confirmed", tone: "attention" };
    case "running":
      return { label: "Running", tone: "progress" };
    case "done":
      return { label: "Done", tone: "complete" };
    case "spent":
      return { label: "Spent", tone: "attention" };
    case "stopped":
      return { label: "Stopped", tone: "plain" };
    case "failed":
      return { label: "Failed", tone: "error" };
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
  if (experiment.kept) return { label: "Kept", tone: "complete" };
  return experiment.value === null
    ? { label: "Failed", tone: "error" }
    : { label: "Not kept", tone: "plain" };
}

/** The brief being written, with the repository it is for. */
export interface Form {
  repository: number | null;
  goal: string;
  measure: string;
  better: Better;
  /** What the budget counts. */
  kind: BudgetKind;
  /** The amount of the budget, as it was typed. */
  budget: string;
}

export function emptyForm(repository: number | null = null): Form {
  return {
    repository,
    goal: "",
    measure: "",
    better: "lower",
    kind: "tokens",
    budget: DEFAULT_BUDGET,
  };
}

/** The form with another kind of budget: an amount still at the default of the kind before becomes that of the new one. */
export function withKind(form: Form, kind: BudgetKind): Form {
  if (form.kind === kind) return form;
  const untouched = form.budget.trim() === defaultAmount(form.kind);
  return { ...form, kind, budget: untouched ? defaultAmount(kind) : form.budget };
}

export function formOf(research: Research): Form {
  return {
    repository: research.repository_id,
    goal: research.goal,
    measure: research.measure,
    better: research.better === "higher" ? "higher" : "lower",
    kind: research.budget.kind,
    budget: String(research.budget.amount),
  };
}

/** The amount of the budget, if what was typed is a whole number above 0. */
export function budgetOf(form: Form): number | null {
  const budget = Number(form.budget.trim());
  return form.budget.trim() !== "" && Number.isInteger(budget) && budget > 0 ? budget : null;
}

/**
 * The starting points offered on a page with no research yet: a number worth improving and a command that prints it,
 * which the person edits into their own brief.
 */
export const SUGGESTED_BRIEFS: Array<Pick<Form, "goal" | "measure" | "better">> = [
  {
    goal: "Cut the clean build time",
    measure: "/usr/bin/time -f %e cargo build --release --quiet 2>&1 | tail -1",
    better: "lower",
  },
  {
    goal: "Cut the test run time",
    measure: "/usr/bin/time -f %e cargo test --quiet 2>&1 | tail -1",
    better: "lower",
  },
  {
    goal: "Cut the lint warnings",
    measure: "cargo clippy --message-format=json 2>&1 | grep -c warning",
    better: "lower",
  },
];

/** The brief to send, when the form has a repository, a goal, a measure and a budget. */
export function briefOf(form: Form): Brief | null {
  const budget = budgetOf(form);
  if (form.repository === null || budget === null) return null;
  if (form.goal.trim() === "" || form.measure.trim() === "") return null;
  return {
    goal: form.goal.trim(),
    measure: form.measure.trim(),
    better: form.better,
    budget: { kind: form.kind, amount: budget },
  };
}

/** What a research has used of its budget: `4 of 10 experiments` or `120k of 500k tokens`. */
export function usage(research: Pick<Research, "budget" | "used" | "tokens">): string {
  const { kind, amount } = research.budget;
  if (kind === "experiments") {
    return `${research.used?.experiments ?? 0} of ${amount} experiments`;
  }
  return `${short(research.used?.tokens ?? research.tokens ?? 0)} of ${short(amount)} tokens`;
}

/** The limit a spent research reached, in words; none for a research that is not spent. */
export function limitReached(research: Pick<Research, "limit_reached" | "budget">): string | null {
  if (!research.limit_reached) return null;
  const budget: Budget = research.budget;
  return research.limit_reached === "experiments"
    ? `The limit of ${budget.amount} experiments was reached.`
    : `The limit of ${short(budget.amount)} tokens was reached.`;
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
