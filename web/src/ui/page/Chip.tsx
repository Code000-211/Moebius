/** A filter chip: a pill that is pressed or not. `red` marks what is blocked (the Stuck kind of the inbox). */
import type { ReactNode } from "react";
import { cn } from "../lib/cn";

export function Chip({
  selected,
  red = false,
  onClick,
  children,
}: {
  selected: boolean;
  red?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-pressed={selected}
      onClick={onClick}
      className={cn(
        "rounded-full border px-[12px] py-half text-[12.5px] transition-colors",
        selected
          ? red
            ? "border-[var(--red-border)] bg-red-fill text-red"
            : "border-accent-border bg-accent-fill text-accent"
          : "border-line bg-surface text-normal hover:text-high",
      )}
    >
      {children}
    </button>
  );
}
