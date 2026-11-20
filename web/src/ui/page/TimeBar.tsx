/**
 * A bar on a time axis: it starts where its first day falls on the axis, ends where its last day does, and fills from
 * its left edge as far as its progress. Several bars given the same axis line up as rows of one timeline, and a thin
 * line marks today where it falls inside the axis. The Cycles page uses it for each cycle; the days are `YYYY-MM-DD`
 * and both ends of the axis and of the bar count as whole days.
 *
 * ```tsx
 * <TimeBar start="2026-11-02" end="2026-11-15" axis={{ start: "2026-11-01", end: "2026-12-01" }} progress={40} />
 * ```
 */
import { cn } from "../lib/cn";

const DAY = 86_400_000;

/** The number of whole days from the epoch to `iso`, which is `YYYY-MM-DD`. */
function dayNumber(iso: string): number {
  return Math.floor(Date.parse(`${iso}T00:00:00Z`) / DAY);
}

/** Where a bar from `start` to `end` lies on an axis, as percent of the axis from its left edge and of its width. */
export function placeOnAxis(
  start: string,
  end: string,
  axis: { start: string; end: string },
): { left: number; width: number } {
  const from = dayNumber(axis.start);
  const length = Math.max(1, dayNumber(axis.end) - from + 1);
  const first = Math.min(Math.max(dayNumber(start), from), from + length - 1);
  const last = Math.max(Math.min(dayNumber(end), from + length - 1), first);
  return {
    left: ((first - from) / length) * 100,
    width: ((last - first + 1) / length) * 100,
  };
}

/** The axis that holds every `{ start, end }` of a list: from the earliest first day to the latest last day. */
export function axisOf(
  spans: Array<{ start: string; end: string }>,
): { start: string; end: string } | null {
  if (spans.length === 0) return null;
  return {
    start: spans.map((s) => s.start).sort()[0] ?? "",
    end:
      spans
        .map((s) => s.end)
        .sort()
        .reverse()[0] ?? "",
  };
}

export function TimeBar({
  start,
  end,
  axis,
  progress,
  label,
  today,
  emphasis = false,
  className,
}: {
  start: string;
  end: string;
  axis: { start: string; end: string };
  /** Whole percent of the bar that is done. */
  progress: number;
  /** What the bar says to a screen reader, for instance the words of its progress. */
  label?: string;
  /** The day to mark on the axis, when it falls inside it. */
  today?: string;
  /** The bar of the one that is under way: accent instead of grey. */
  emphasis?: boolean;
  className?: string;
}) {
  const { left, width } = placeOnAxis(start, end, axis);
  const todayAt =
    today !== undefined && today >= axis.start && today <= axis.end
      ? placeOnAxis(today, today, axis)
      : null;
  const value = Math.min(100, Math.max(0, progress));
  return (
    <div className={cn("relative h-2", className)} data-testid="time-bar">
      <div
        className="absolute inset-x-0 top-1/2 h-px -translate-y-1/2 bg-hairline"
        aria-hidden="true"
      />
      <div
        role="progressbar"
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={value}
        data-testid="time-bar-span"
        className={cn(
          "absolute inset-y-0 overflow-hidden rounded-full",
          emphasis ? "bg-accent/30" : "bg-hairline-strong",
        )}
        style={{ left: `${left}%`, width: `${width}%` }}
      >
        <div
          className={cn("h-full", emphasis ? "bg-accent" : "bg-status-done")}
          style={{ width: `${value}%` }}
        />
      </div>
      {todayAt && (
        <div
          data-testid="time-bar-today"
          aria-hidden="true"
          className="absolute -inset-y-[14px] w-px bg-accent"
          style={{ left: `${todayAt.left + todayAt.width / 2}%` }}
        />
      )}
    </div>
  );
}
