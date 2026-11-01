// From BloopAI/vibe-kanban packages/ui/src/components/KanbanBadge.tsx (Apache-2.0), changed: color is any CSS colour (a Deep Velvet variable) in place of an HSL triple, and the badge has a tone for the board's states (the original had tags only).
import { cn } from "../lib/cn";

export type KanbanBadgeTone = "plain" | "live" | "teal" | "amber" | "red";

const TONES: Record<KanbanBadgeTone, string> = {
  plain: "bg-panel text-low",
  live: "bg-live-fill text-live",
  teal: "bg-teal-fill text-success",
  amber: "bg-amber-fill text-amber",
  red: "bg-red-fill text-error",
};

export type KanbanBadgeProps = {
  name: string;
  /** Any CSS colour, such as `var(--teal)`, for a dot before the name. */
  color?: string;
  tone?: KanbanBadgeTone;
  className?: string;
};

export const KanbanBadge = ({ name, color, tone = "plain", className }: KanbanBadgeProps) => {
  return (
    <span
      className={cn(
        "inline-flex items-center justify-center",
        "h-5 px-base gap-half",
        "rounded-sm",
        "text-xs font-medium",
        "whitespace-nowrap",
        TONES[tone],
        className,
      )}
    >
      {color && (
        <span className="w-2 h-2 rounded-full shrink-0" style={{ backgroundColor: color }} />
      )}
      {name}
    </span>
  );
};
