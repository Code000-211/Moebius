/**
 * A rules file as text: a rule's `## name` section put back where it was, appended or dropped, what a rule's name may
 * be, and the request to the Moebius agent. The logic of `screens/rules/file.rs` of the desktop app; a save sends the
 * whole file, so the page works on its text and the server checks it.
 */

import { plural } from "../../ui/lib/format";

/** The name of the rule a line starts, if it is a `## name` heading. */
function heading(line: string): string | undefined {
  return line.startsWith("## ") ? line.slice(3).trim() : undefined;
}

/** The parts that are not blank, one blank line apart, ending with a line break; empty when all are blank. */
function joined(parts: string[]): string {
  const kept = parts.map((part) => part.trimEnd()).filter((part) => part.trim() !== "");
  return kept.length === 0 ? "" : `${kept.join("\n\n")}\n`;
}

/**
 * `file` with the `## name` section replaced by `section`, or dropped when `section` is blank. Null when the file has
 * no rule of that name.
 */
export function replaceRule(file: string, name: string, section: string): string | null {
  const lines = file.split("\n");
  if (lines.at(-1) === "") lines.pop();
  const start = lines.findIndex((line) => heading(line) === name);
  if (start < 0) return null;
  const next = lines.slice(start + 1).findIndex((line) => heading(line) !== undefined);
  const end = next < 0 ? lines.length : start + 1 + next;
  return joined([lines.slice(0, start).join("\n"), section, lines.slice(end).join("\n")]);
}

/** `file` with `section` added at its end. */
export function appendRule(file: string, section: string): string {
  return joined([file, section]);
}

/** A rule as its section of a rules file. */
export function section(name: string, text: string): string {
  return `## ${name.trim()}\n${text.trimEnd()}`;
}

/** Whether `name` can be a rule's name: not blank, without spaces or line breaks. */
export function validName(name: string): boolean {
  const trimmed = name.trim();
  return trimmed !== "" && !/\s/.test(trimmed);
}

/** What the Moebius agent is asked, with what the person said. */
export function agentRequest(wanted: string): string {
  return [
    'Change the server\'s rules in prose. Read the current rules with `"$MOEBIUS" rules` and the rules guide of the moebius skill first. Write the change as rules in prose, each a `## name` heading and a few plain sentences, keep every existing rule as it is unless the person asks to change or remove it, and never weaken a rule on your own. Write the whole rules file to a file and save it with `"$MOEBIUS" rules save <file>`. Then say what changed.',
    `What should be allowed or forbidden:\n\n${wanted}`,
  ].join("\n\n");
}

/** How often a rule refused a call in the last 7 days, as the tag says it. */
export function refusalsLabel(refusals: Record<string, number>, name: string): string {
  const n = refusals[name] ?? 0;
  if (n === 0) return "No refusals this week";
  return plural(n, "refusal this week", "refusals this week");
}
