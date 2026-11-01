/**
 * The words of the Retro page, as `screens/retro/` of the desktop app says them: the kinds of error of a report in the
 * order the page lists them, the tags of a Retro, a proposal and a lesson, and the lines of a proposal and its trials.
 */

import type {
  Lesson,
  Proposal,
  ProposalStatus,
  ReportError,
  RetroStatus,
  Trial,
} from "../../api/types/improve";
import type { TagTone } from "../../ui/chat/Tag";

/** The kinds of error of a report, in the order the page lists them, with what the page calls them. */
export const KINDS: ReadonlyArray<readonly [kind: string, label: string]> = [
  ["tool_failed", "Failed tool calls"],
  ["tool_refused", "Refused tool calls"],
  ["run_failed", "Failed runs"],
  ["run_stopped", "Stopped runs"],
  ["steered", "Steering"],
  ["checks_failed", "Failed checks"],
  ["conflict", "Merge conflicts"],
  ["review_findings", "Review findings"],
];

export interface TagSpec {
  label: string;
  tone: TagTone;
}

export function retroTag(status: RetroStatus): TagSpec {
  switch (status) {
    case "analysing":
      return { label: "Analysing", tone: "amber" };
    case "proposed":
      return { label: "Proposed", tone: "teal" };
    case "failed":
      return { label: "Failed", tone: "red" };
    default:
      return { label: "Reported", tone: "plain" };
  }
}

export function plural(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`;
}

/** Where an error happened: its card, run and agent. */
export function origin(error: ReportError): string {
  const parts: string[] = [];
  if (error.card !== null) parts.push(`card ${error.card}`);
  if (error.run !== null) parts.push(`run ${error.run}`);
  if (error.agent) parts.push(error.agent);
  return parts.join(" · ");
}

/** The errors of a report grouped by kind, in the order of `KINDS`; kinds without an error are left out. */
export function groupErrors(
  errors: ReportError[],
): Array<{ kind: string; label: string; errors: ReportError[] }> {
  return KINDS.map(([kind, label]) => ({
    kind,
    label,
    errors: errors.filter((error) => error.kind === kind),
  })).filter((group) => group.errors.length > 0);
}

/** A proposal's status as a tag: how far its trials got, and what a person did with it. */
export function proposalTag(status: ProposalStatus): TagSpec {
  switch (status) {
    case "trying":
      return { label: "Trying", tone: "amber" };
    case "positive":
      return { label: "Positive", tone: "teal" };
    case "negative":
      return { label: "Negative", tone: "red" };
    case "untried":
      return { label: "Untried", tone: "amber" };
    case "merged":
      return { label: "Merged", tone: "teal" };
    case "dismissed":
      return { label: "Dismissed", tone: "plain" };
    default:
      return { label: "Open", tone: "plain" };
  }
}

/** What a proposal for Moebius's own configuration changes, as its tag says it; none for one of the harness. */
export function ownKind(kind: Proposal["kind"]): string | null {
  if (kind === "settings") return "moebius.toml";
  if (kind === "repository") return ".moebius/";
  return null;
}

/** The cards of a proposal as the page says them: `Cards 48, 50`. */
export function cardsLine(cards: number[]): string {
  return `Cards ${cards.join(", ")}`;
}

/** One trial: its outcome with its tone, and the card it was tried on. */
export function trialLine(trial: Trial): { outcome: TagSpec; card: string } {
  const tone: TagTone =
    trial.outcome === "positive" ? "teal" : trial.outcome === "negative" ? "red" : "plain";
  const card =
    trial.card_id === null
      ? "No card"
      : trial.regression
        ? `Regression card ${trial.card_id}`
        : `Card ${trial.card_id}`;
  return {
    outcome: { label: trial.outcome.charAt(0).toUpperCase() + trial.outcome.slice(1), tone },
    card,
  };
}

/** A lesson's kind as a tag. */
export function lessonTag(kind: Lesson["kind"]): TagSpec {
  if (kind === "success") return { label: "Worked", tone: "teal" };
  if (kind === "rejection") return { label: "Rejected", tone: "amber" };
  return { label: "Failed", tone: "red" };
}

/** The mono line under a lesson: its cards, how often it was given, and why it was retired. */
export function lessonMeta(lesson: Lesson): string {
  let meta = "";
  if (lesson.cards && lesson.cards.length > 0) meta = `Cards ${lesson.cards.join(", ")} · `;
  meta += `given ${lesson.retrievals ?? 0} times`;
  if (lesson.retired_reason) meta += ` · ${lesson.retired_reason}`;
  return meta;
}
