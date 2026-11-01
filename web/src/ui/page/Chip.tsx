/** A filter: a pill that is pressed or not, at least 40px tall, with the status colour only where it means something. */
import type { ReactNode } from "react";
import { cn } from "../lib/cn";

export function Chip({
  selected,
  tone = "neutral",
  onClick,
  children,
  className,
}: {
  /** Whether this chip is the one that is on. */
  selected: boolean;
  /** The state the chip filters to, when that is one of the four the palette carries. */
  tone?: "neutral" | "progress" | "attention" | "complete" | "error";
  onClick: () => void;
  children: ReactNode;
  className?: string;
}) {
  const on = {
    neutral: "bg-selected text-text",
    progress: "bg-progress-fill text-progress-text",
    attention: "bg-attention-fill text-attention-text",
    complete: "bg-complete-fill text-complete-text",
    error: "bg-error-fill text-error-text",
  }[tone];
  return (
    <button
      type="button"
      aria-pressed={selected}
      onClick={onClick}
      className={cn(
        "inline-flex min-h-10 cursor-pointer items-center gap-2 rounded-full px-4 text-sm font-semibold transition-colors duration-(--motion-routine) ease-out",
        selected ? on : "bg-sunken text-secondary hover:bg-hover hover:text-text",
        className,
      )}
    >
      {children}
    </button>
  );
}
