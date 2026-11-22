/**
 * What starts a workflow, in words: the chip beside its name (`Webhook`, `Schedule · next 08:00`, `Manual`) and the
 * trigger's own line on the Overview. A schedule's next firing is the server's (`next_firing` of the list's row, worked
 * out from the cron in its own local time); nothing here reads a cron expression, so a workflow with no next firing
 * says so rather than a guess.
 */
import type { Trigger, WorkflowSummary } from "../../api/types/workflows";
import { triggerName } from "./model";

const MINUTE = 60;

/** The clock time of `at` (Unix seconds) in the person's zone, with the day when it is not today: `08:00`, `tomorrow 08:00`, `Mon 08:00`. */
export function firingAt(now: number, at: number): string {
  const when = new Date(at * 1000);
  const clock = when.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false });
  const today = new Date(now * 1000);
  const days = Math.round(
    (Date.UTC(when.getFullYear(), when.getMonth(), when.getDate()) -
      Date.UTC(today.getFullYear(), today.getMonth(), today.getDate())) /
      86_400_000,
  );
  if (at - now < MINUTE && at >= now) return `${clock} (now)`;
  if (days <= 0) return clock;
  if (days === 1) return `tomorrow ${clock}`;
  const weekday = when.toLocaleDateString([], { weekday: "short" });
  return days < 7
    ? `${weekday} ${clock}`
    : `${when.toLocaleDateString([], { day: "numeric", month: "short" })} ${clock}`;
}

/** The chip of a workflow's header: what starts it, and for a schedule when it fires next. */
export function triggerChip(
  kind: string | undefined,
  summary: WorkflowSummary | undefined,
  now: number,
): string {
  const name = triggerName(kind);
  if (kind === "schedule" && summary?.next_firing !== undefined) {
    return `${name} · next ${firingAt(now, summary.next_firing)}`;
  }
  return name;
}

/** The cron of a schedule trigger, when it names one. */
export function cronOf(trigger: Trigger | undefined): string | null {
  const cron = trigger?.params?.cron;
  return typeof cron === "string" && cron.trim() !== "" ? cron : null;
}
