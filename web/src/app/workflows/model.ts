/**
 * The words the Workflows pages use for the states the server sends: a trigger's kind, a run's status and a node's
 * status, each as the label a person reads and the tone it wears. They live here so the list, the definition and the
 * run cannot drift apart in what they call the same thing.
 */

import type { WorkflowRunSummary, WorkflowTrigger } from "../../api/workflows";
import type { TagTone } from "../../ui/chat/Tag";

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
    case "done":
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
