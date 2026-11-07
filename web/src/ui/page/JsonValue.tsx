/**
 * One value shown as JSON, with a long one cut short until "Show all" is pressed. It is what the run page uses for a
 * node's input, its output and what a waiting node asks, so a redacted value keeps its redaction mark as the string the
 * server sent, and a value too large to read at a glance cannot push the rest of the page away.
 */
import { useState } from "react";
import { Button } from "../components/Button";

/** How much of a value is shown before it is cut, in characters: about a screen of monospaced text. */
const SHOWN = 900;

/** A value as JSON. A string the server sent is quoted, so it reads as a JSON value and not as prose. */
export function asJson(value: unknown): string {
  if (value === undefined) return "null";
  try {
    return JSON.stringify(value, null, 2) ?? "null";
  } catch {
    return String(value);
  }
}

export function JsonValue({ value, name }: { value: unknown; name?: string }) {
  const [all, setAll] = useState(false);
  const text = asJson(value);
  const long = text.length > SHOWN;
  const shown = long && !all ? `${text.slice(0, SHOWN)}\n…` : text;
  return (
    <div className="flex min-w-0 flex-col gap-2">
      <pre
        className="m-0 overflow-x-auto whitespace-pre-wrap break-words rounded-control bg-code px-3 py-2 font-mono text-xs leading-5 text-text"
        data-json={name}
        data-complete={long ? String(all) : "true"}
      >
        {shown}
      </pre>
      {long ? (
        <div>
          <Button size="sm" variant="secondary" aria-expanded={all} onClick={() => setAll(!all)}>
            {all ? "Show less" : `Show all (${text.length} characters)`}
          </Button>
        </div>
      ) : null}
    </div>
  );
}
