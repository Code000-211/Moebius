/**
 * One state, said in a word and carried by its colour. Every page that shows what a thing is doing needs this — the
 * board's badges, a run's standing, a runner's state — and the standard is the same everywhere: the hue is never the
 * only signal, so the word is required, and the pill is never a control.
 */
import type { ReactNode } from "react";
import { cn } from "../lib/cn";

/** The four states the palette carries, plus a neutral for a fact that is not a state. */
export type Tone = "neutral" | "progress" | "attention" | "complete" | "error";

const TONES: Record<Tone, string> = {
  neutral: "bg-sunken text-secondary",
  progress: "bg-progress-fill text-progress-text",
  attention: "bg-attention-fill text-attention-text",
  complete: "bg-complete-fill text-complete-text",
  error: "bg-error-fill text-error-text",
};

export function Status({
  tone = "neutral",
  children,
  className,
}: {
  /** The state the colour stands for; the word beside it is the same state in plain language. */
  tone?: Tone;
  children: ReactNode;
  className?: string;
}) {
  return (
    <span
      className={cn(
        "inline-flex items-center gap-2 break-words rounded-full px-3 py-1 text-sm font-semibold",
        TONES[tone],
        className,
      )}
    >
      {children}
    </span>
  );
}
