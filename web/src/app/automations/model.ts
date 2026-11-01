/**
 * The logic of the Automations page, as `screens/automations/mod.rs` of the desktop app has it: the schedules the
 * picker offers, how long until a firing, what a save sends and what is wrong with the fields, where a chat works,
 * and the line under the schedule that says when it fires next.
 */
import type { Automation, AutomationSettings } from "../../api/types/automations";

/** The schedules the picker offers, with what they are called. */
export const PRESETS: ReadonlyArray<readonly [name: string, cron: string]> = [
  ["Every hour", "0 * * * *"],
  ["Every morning at 8", "0 8 * * *"],
  ["Weekdays at 8", "0 8 * * 1-5"],
  ["Every Friday at 9", "0 9 * * 5"],
  ["Every night at 2", "0 2 * * *"],
  ["Every Monday at 9", "0 9 * * 1"],
];

/** What a schedule is called: the name of its preset, or the expression itself. */
export function scheduleName(schedule: string): string {
  const trimmed = schedule.trim();
  return PRESETS.find(([, cron]) => cron === trimmed)?.[0] ?? trimmed;
}

/** How long until `at`, at `now` (both Unix seconds): `in 6 min`, `in 3 h`, `in 2 d`. */
export function until(now: number, at: number): string {
  const seconds = Math.max(0, at - now);
  if (seconds < 60) return "in under a minute";
  if (seconds < 3600) return `in ${Math.floor(seconds / 60)} min`;
  if (seconds < 86_400) return `in ${Math.floor(seconds / 3600)} h`;
  return `in ${Math.floor(seconds / 86_400)} d`;
}

/** Where an automation's chats work: an empty directory, a repository (its id) or the harness repository. */
export type Place = "nowhere" | "harness" | `repository:${number}`;

export function placeOf(automation: Automation): Place {
  if (automation.repository_id !== null) return `repository:${automation.repository_id}`;
  return automation.harness_repository ? "harness" : "nowhere";
}

/** The fields of the editor. */
export interface Form {
  name: string;
  schedule: string;
  agent: string;
  place: Place;
  prompt: string;
  enabled: boolean;
}

/** The form of a new automation: every morning at 8, with the Moebius agent, in an empty directory. */
export function newForm(): Form {
  return {
    name: "",
    schedule: PRESETS[1]?.[1] ?? "0 8 * * *",
    agent: "Moebius",
    place: "nowhere",
    prompt: "",
    enabled: true,
  };
}

export function formOf(automation: Automation): Form {
  return {
    name: automation.name,
    schedule: automation.schedule,
    agent: automation.agent,
    place: placeOf(automation),
    prompt: automation.prompt,
    enabled: automation.enabled,
  };
}

/** What a save sends, or what is wrong with the fields. */
export function request(form: Form): { ok: AutomationSettings } | { problem: string } {
  const prompt = form.prompt.trim();
  if (form.name.trim() === "") return { problem: "Give the automation a name." };
  if (form.schedule.trim().split(/\s+/).length !== 5) {
    return {
      problem:
        "A schedule has five fields: minute, hour, day of the month, month, day of the week.",
    };
  }
  if (prompt === "") return { problem: "Write the prompt its chats start with." };
  return {
    ok: {
      name: form.name.trim(),
      schedule: form.schedule.trim(),
      agent: form.agent,
      prompt,
      repository: form.place.startsWith("repository:") ? Number(form.place.slice(11)) : null,
      harness: form.place === "harness",
      enabled: form.enabled,
    },
  };
}

/** The line under the schedule: when it fires next, as far as the server knows. */
export function nextLine(
  saved: Automation | undefined,
  schedule: string,
  enabled: boolean,
  now: number,
): string {
  if (!saved) return "Fires once saved, when the schedule comes due.";
  if (saved.schedule.trim() !== schedule.trim() || saved.enabled !== enabled) {
    return "Save to see when it fires next.";
  }
  if (saved.next_firing !== null) return `Fires next ${until(now, saved.next_firing)}.`;
  return saved.enabled ? "This schedule never comes due." : 'Off: only "Run now" fires it.';
}
