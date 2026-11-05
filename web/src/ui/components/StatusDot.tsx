// From BloopAI/vibe-kanban packages/ui/src/components/StatusDot.tsx (Apache-2.0), changed: a status is never colour
// alone — the caller still has the word, beside the mark or in the name of the row it stands in — and `color` takes
// one of the status tokens of styles/theme.css.
import { cn } from "../lib/cn";

export interface StatusDotProps {
  /** For a test to find the mark by. */
  "data-testid"?: string;
  /** A status colour of the theme, such as `var(--complete)`. */
  color: string;
  className?: string;
}

export const StatusDot = ({ color, className, "data-testid": testId }: StatusDotProps) => (
  <span
    data-testid={testId}
    aria-hidden="true"
    className={cn("size-2 shrink-0 rounded-full", className)}
    style={{ backgroundColor: color }}
  />
);
