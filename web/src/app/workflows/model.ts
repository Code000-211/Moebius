/**
 * The words the Workflows tab and its pages use for the states the server sends: a trigger's kind, a run's status and a node's
 * status, each as the label a person reads and the tone it wears. They live here so the list, the definition and the
 * run cannot drift apart in what they call the same thing. Also the shape test of the server (`is_automation`), which
 * the pages need to say when a workflow is one of the Automations page's entries rather than a plain workflow.
 */

import type { Automation } from "../../api/types/automations";
import type { LastRun, WorkflowDefinition, WorkflowSummary } from "../../api/types/workflows";
import type { TagTone } from "../../ui/chat/Tag";
import { ago } from "../../ui/lib/format";
import { until } from "../automations/model";

/** The trigger's kind as a word: what starts the workflow. */
export function triggerName(kind: string | null | undefined): string {
  switch (kind) {
    case undefined:
    case null:
    case "":
      return "No trigger";
    case "manual":
      return "Manual";
    case "schedule":
      return "Schedule";
    case "event":
      return "Event";
    case "webhook":
      return "Webhook";
    default:
      return kind;
  }
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
export function lastRunLine(last: LastRun | null): string {
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
    definition.trigger[0]?.kind === "schedule" &&
    definition.nodes.length === 1 &&
    definition.nodes[0]?.kind === "chat"
  );
}

/** The filters of the runs of a project: all of them, those a schedule started, those that failed. */
export type RunFilter = "all" | "scheduled" | "failed";

/** The filter an address names (`?runs=`); anything else is all the runs. */
export function runFilter(asked: string | null): RunFilter {
  return asked === "scheduled" || asked === "failed" ? asked : "all";
}

/** Whether `name` is the file an automation is kept in: its own name, or `automation-<id>` when that name is no file's. */
function keeps(automation: Automation, name: string): boolean {
  return name === automation.name || new RegExp(`^automation-${automation.id}(-\\d+)?$`).test(name);
}

/**
 * The workflows a schedule starts, each with the automation that is it when it is one (a schedule that starts one chat)
 * and so knows its schedule and when it fires next; a schedule that starts something else has no automation row.
 */
export function scheduledWorkflows(
  workflows: readonly WorkflowSummary[],
  automations: readonly Automation[],
): Array<{ workflow: WorkflowSummary; automation?: Automation }> {
  return workflows
    .filter((each) => each.trigger_kind === "schedule")
    .map((workflow) => {
      const automation = automations.find((each) => keeps(each, workflow.name));
      return automation ? { workflow, automation } : { workflow };
    });
}

/**
 * What the Scheduled list says of a schedule-trigger workflow that is not an automation: when it fires next, which the
 * server works out from the trigger's cron in its own local time, and when it last fired. Without a next firing the
 * schedule is off or never comes due, and the line says that rather than guess.
 */
export function scheduleLine(workflow: WorkflowSummary, now: number): string {
  const last =
    workflow.last_fired === undefined ? "" : ` · last fired ${ago(now, workflow.last_fired)}`;
  return workflow.next_firing === undefined
    ? `Schedule · no next firing${last}`
    : `Schedule · next ${until(now, workflow.next_firing)}${last}`;
}
