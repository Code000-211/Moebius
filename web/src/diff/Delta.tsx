/**
 * What a diff adds and removes, in the one shape every page uses: `+21 -2`. The sign says which number is which
 * before the colour does, and the words are still there for whoever listens to the page rather than looks at it.
 */
import { cn } from "../ui/lib/cn";

export function Delta({
  additions,
  deletions,
  className,
}: {
  additions: number;
  deletions: number;
  className?: string;
}) {
  return (
    <span className={cn("shrink-0 tabular-nums", className)}>
      <span className="text-complete-text">
        +{additions}
        <span className="sr-only"> added</span>
      </span>
      <span className="text-secondary"> </span>
      <span className="text-error-text">
        -{deletions}
        <span className="sr-only"> removed</span>
      </span>
    </span>
  );
}
