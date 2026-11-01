/**
 * The body of a changed file: the removed lines in red and the added ones in green, from the old and new text of an
 * edit (a write is all added). The stream carries the texts, not a unified diff, so no context lines are drawn.
 * At most `MAX_LINES` lines are drawn; the rest is said to be left out.
 */
import { cn } from "../lib/cn";
import { CopyButton } from "./CopyButton";

export interface DiffChange {
  oldText: string;
  newText: string;
}

const MAX_LINES = 200;

export interface DiffLine {
  sign: "+" | "-";
  text: string;
}

/** The lines of the changes, removed ones first in each change. */
export function diffLines(changes: DiffChange[]): DiffLine[] {
  const lines: DiffLine[] = [];
  for (const change of changes) {
    if (change.oldText !== "") {
      for (const text of change.oldText.split("\n")) lines.push({ sign: "-", text });
    }
    if (change.newText !== "") {
      for (const text of change.newText.split("\n")) lines.push({ sign: "+", text });
    }
  }
  return lines;
}

/** How many lines the changes add and remove. */
export function diffStats(changes: DiffChange[]): { additions: number; deletions: number } {
  const lines = diffLines(changes);
  return {
    additions: lines.filter((line) => line.sign === "+").length,
    deletions: lines.filter((line) => line.sign === "-").length,
  };
}

export function DiffBody({ changes }: { changes: DiffChange[] }) {
  const lines = diffLines(changes);
  const shown = lines.slice(0, MAX_LINES);
  const text = lines.map((line) => `${line.sign}${line.text}`).join("\n");
  return (
    <div className="border-t bg-recessed">
      <div className="flex justify-end px-base pt-half">
        <CopyButton text={text} label="Copy diff" />
      </div>
      <pre className="overflow-x-auto px-base pb-base font-mono text-xs leading-5">
        {shown.map((line, index) => (
          <div
            // biome-ignore lint/suspicious/noArrayIndexKey: the lines are drawn once, in order
            key={index}
            className={cn(
              "whitespace-pre",
              line.sign === "+" ? "bg-success/10 text-success" : "bg-error/10 text-error",
            )}
          >
            {line.sign}
            {line.text}
          </div>
        ))}
        {lines.length > shown.length && (
          <div className="text-low">… {lines.length - shown.length} more lines left out</div>
        )}
      </pre>
    </div>
  );
}
