/**
 * A short label that states a state or a place: the status of a chat, the agent it is with, the repository it works
 * in. It is a label and not a control, so it is words at caption size on a tint, never a coloured mark on its own, and
 * it wraps when the words are long.
 *
 * A tone names the state, never the colour: `progress`, `attention`, `complete` and `error` are the four the palette
 * carries, and `plain` is for a label that states a fact rather than a state.
 */
import { cn } from "../lib/cn";

export type TagTone = "plain" | "progress" | "attention" | "complete" | "error";

const TONES: Record<TagTone, string> = {
  plain: "bg-sunken text-secondary",
  progress: "bg-progress-fill text-progress-text",
  attention: "bg-attention-fill text-attention-text",
  complete: "bg-complete-fill text-complete-text",
  error: "bg-error-fill text-error-text",
};

export function Tag({
  tone = "plain",
  className,
  children,
}: {
  tone?: TagTone;
  className?: string;
  children: React.ReactNode;
}) {
  return (
    <span
      className={cn(
        "inline-flex max-w-full items-center gap-2 break-words rounded-full px-2 py-1 text-xs font-semibold",
        TONES[tone],
        className,
      )}
    >
      {children}
    </span>
  );
}
