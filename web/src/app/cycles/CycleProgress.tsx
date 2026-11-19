import type { CycleView } from "../../api/types/cycles";
import { cn } from "../../ui/lib/cn";
import { percent, progressWords } from "./model";

export const STATE_WORDS = {
  closed: "Closed",
  upcoming: "Upcoming",
  running: "Running",
  overdue: "Past its end",
} as const;

/** The bar of progress: the part done, with the words the bar stands for as its name. */
export function Progress({ cycle, className }: { cycle: CycleView; className?: string }) {
  const value = percent(cycle);
  return (
    <div
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={value}
      aria-label={progressWords(cycle)}
      className={cn("h-1.5 overflow-hidden rounded-full bg-sunken", className)}
    >
      <div className="h-full bg-complete" style={{ width: `${value}%` }} />
    </div>
  );
}
