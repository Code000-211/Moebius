/**
 * What the server answers to a script it refuses (`PUT /v1/repositories/{id}/scripts/{column}`, 422): the message of
 * `moebius check-script`, such as `syntax error: [string "ready"]:4: Expected ')' ...` or, for a failure while the
 * script runs, `runtime error: [string "ready"]:2: attempt to index nil` and a stack traceback. This finds the line
 * the check names, so the editor can mark it.
 */

export interface ScriptProblem {
  /** The line the check names, counted from 1; null when the message names none. */
  line: number | null;
  /** The message without the chunk name and line, which the editor shows at the line. */
  message: string;
  /** The whole answer, with the traceback. */
  full: string;
}

/** The first `...]:<line>:` of the message is the script's own line (a traceback repeats it). */
export function scriptProblem(answer: string): ScriptProblem {
  const full = answer.trim();
  const first = full.split("\n")[0] ?? "";
  const found = /\]:(\d+):\s*(.*)$/.exec(first);
  if (!found) return { line: null, message: first, full };
  const kind = /^(syntax error|runtime error)/.exec(first)?.[1];
  const message = (kind ? `${kind}: ` : "") + (found[2] ?? "");
  return { line: Number(found[1]), message, full };
}
