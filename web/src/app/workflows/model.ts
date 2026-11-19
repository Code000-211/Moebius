/**
 * The words the Workflows pages use for the states the server sends: a trigger's kind, a run's status and a node's
 * status, each as the label a person reads and the tone it wears. They live here so the list, the definition and the
 * run cannot drift apart in what they call the same thing. Also the shape test of the server (`is_automation`), which
 * the pages need to say when a workflow is one of the Automations page's entries rather than a plain workflow.
 */

import type {
  WorkflowDefinition,
  WorkflowRunSummary,
  WorkflowSummary,
  WorkflowTrigger,
} from "../../api/workflows";
import type { TagTone } from "../../ui/chat/Tag";
import type { FilterField } from "../../ui/filters/model";

/** The trigger in a few words: what starts it, and the schedule or event when it has one. */
export function triggerName(trigger: WorkflowTrigger | null | undefined): string {
  if (!trigger) return "No trigger";
  const kind =
    trigger.kind === "manual"
      ? "Manual"
      : trigger.kind === "schedule"
        ? "Schedule"
        : trigger.kind === "event"
          ? "Event"
          : trigger.kind === "webhook"
            ? "Webhook"
            : trigger.kind;
  return trigger.detail ? `${kind} · ${trigger.detail}` : kind;
}

/** A run's status as its label and tone. A run that waits, or failed, is the one worth a hue of its own. */
export function runStatus(status: string): { label: string; tone: TagTone } {
  switch (status) {
    case "queued":
      return { label: "Queued", tone: "plain" };
    case "running":
      return { label: "Running", tone: "progress" };
    case "waiting":
      return { label: "Waiting", tone: "attention" };
    case "succeeded":
      return { label: "Done", tone: "complete" };
    case "failed":
      return { label: "Failed", tone: "error" };
    case "cancelled":
      return { label: "Cancelled", tone: "plain" };
    case "skipped":
      return { label: "Skipped", tone: "plain" };
    default:
      return { label: status, tone: "plain" };
  }
}

/** The last run of a workflow, as the line under its name in the list. */
export function lastRunLine(last: WorkflowRunSummary | null): string {
  if (!last) return "Never run";
  return `Last run: ${runStatus(last.status).label.toLowerCase()}`;
}

/** Whether a workflow or run should be marked as needing attention. */
export function needsAttention(status: string | null | undefined): boolean {
  return status === "failed" || status === "waiting";
}

/** Whether the workflow is an automation, the way the server's `is_automation` sees it: a schedule that starts one chat. */
export function looksLikeAutomation(definition: WorkflowDefinition): boolean {
  return (
    definition.trigger.kind === "schedule" &&
    definition.nodes.length === 1 &&
    definition.nodes[0]?.kind === "chat"
  );
}

/** The value of the "Last run" filter for a workflow that has never run. */
export const NEVER_RUN = "never";

/** The keys of the list's filters in the address, in the order the bar offers them. */
export const FILTER_KEYS = ["trigger", "last"] as const;

/** The fields the list is filtered by: what starts a workflow, and how its last run went. */
export function filterFields(
  list: readonly WorkflowSummary[],
  values: Readonly<Record<string, string>>,
): FilterField[] {
  const triggers = new Set(list.map((each) => each.trigger.kind));
  const last = new Set(list.map((each) => each.last_run?.status ?? NEVER_RUN));
  if (values.trigger) triggers.add(values.trigger);
  if (values.last) last.add(values.last);
  return [
    {
      key: "trigger",
      label: "Trigger",
      options: [...triggers]
        .sort()
        .map((kind): [string, string] => [kind, triggerName({ kind, detail: null })]),
    },
    {
      key: "last",
      label: "Last run",
      options: [...last]
        .sort()
        .map((status): [string, string] => [
          status,
          status === NEVER_RUN ? "Never run" : runStatus(status).label,
        ]),
    },
  ];
}

/** The workflows that pass the filters; a filter that is not set lets everything through. */
export function filtered(
  list: readonly WorkflowSummary[],
  values: Readonly<Record<string, string>>,
): WorkflowSummary[] {
  return list.filter(
    (each) =>
      (!values.trigger || each.trigger.kind === values.trigger) &&
      (!values.last || (each.last_run?.status ?? NEVER_RUN) === values.last),
  );
}
