// From BloopAI/vibe-kanban packages/ui/src/components/StatusDot.tsx (Apache-2.0), changed: color is any CSS colour (a Deep Velvet variable), not the HSL triple of Vibe Kanban's tokens.
import { cn } from "../lib/cn";

export interface StatusDotProps {
  /** Any CSS colour, such as `var(--teal)`. */
  color: string;
  className?: string;
}

export const StatusDot = ({ color, className }: StatusDotProps) => (
  <span
    aria-hidden="true"
    className={cn("w-2 h-2 rounded-full shrink-0", className)}
    style={{ backgroundColor: color }}
  />
);
