/**
 * One state of a page, said as a status icon and a word: what every page shows a state with, in place of a coloured pill. The tone
 * picks the icon (progress is the half ring, attention the empty ring, complete the check, error the bar), the icon carries the state's name, and the word beside it says what is
 * meant here. A neutral fact that is not a state has no icon and is plain text.
 */
import type { ReactNode } from "react";
import { cn } from "../lib/cn";
import { StatusIcon, type StatusState } from "./StatusIcon";

/** The four states the palette carries, plus a neutral for a fact that is not a state. */
export type Tone = "neutral" | "progress" | "attention" | "complete" | "error";

const TONE_STATE: Record<Exclude<Tone, "neutral">, StatusState> = {
  progress: "inProgress",
  attention: "notStarted",
  complete: "done",
  error: "blocked",
};

export function StateLabel({
  tone = "neutral",
  children,
  className,
}: {
  tone?: Tone;
  children: ReactNode;
  className?: string;
}) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-2 break-words text-sm font-semibold",
        tone === "neutral" ? "text-secondary" : "text-text",
        className,
      )}
    >
      {tone !== "neutral" && <StatusIcon state={TONE_STATE[tone]} />}
      {children}
    </span>
  );
}
