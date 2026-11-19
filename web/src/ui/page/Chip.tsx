/** A filter: a chip that is pressed or not, 28px tall at the control radius. It is never tinted with the colour of a state: a state is a status icon and a word (`StateLabel`). */
import type { ReactNode } from "react";
import { cn } from "../lib/cn";

export function Chip({
  selected,
  onClick,
  children,
  className,
}: {
  /** Whether this chip is the one that is on. */
  selected: boolean;
  onClick: () => void;
  children: ReactNode;
  className?: string;
}) {
  return (
    <button
      type="button"
      aria-pressed={selected}
      onClick={onClick}
      className={cn(
        "inline-flex min-h-7 cursor-pointer items-center gap-2 rounded-control px-2 text-sm font-medium transition-colors duration-(--motion-routine) ease-out",
        selected
          ? "bg-selected text-text"
          : "bg-sunken text-secondary hover:bg-hover hover:text-text",
        className,
      )}
    >
      {children}
    </button>
  );
}
