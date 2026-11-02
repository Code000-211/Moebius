// From BloopAI/vibe-kanban packages/ui/src/components/PrBadge.tsx (Apache-2.0), changed: the url is optional (GitLab
// and Forgejo pull requests have no address the app knows), then the badge is not a link; the state is a word beside
// the number, so it does not rest on colour; the colours are the status tokens; the link is a 40px target.
import { GitPullRequestIcon } from "@phosphor-icons/react";
import { cn } from "../lib/cn";

export type PrBadgeStatus = "open" | "merged" | "closed";

/** The word and the tint of a pull request: work going on, work that got through, work that ended without one. */
const STATES: Record<PrBadgeStatus, { word: string; tone: string }> = {
  open: { word: "Open", tone: "bg-progress-fill text-progress-text" },
  merged: { word: "Merged", tone: "bg-complete-fill text-complete-text" },
  closed: { word: "Closed", tone: "bg-error-fill text-error-text" },
};

export interface PrBadgeProps {
  number: number;
  url?: string | null;
  status: PrBadgeStatus;
  className?: string;
}

export function PrBadge({ number, url, status, className }: PrBadgeProps) {
  const state = STATES[status];
  const classes = cn(
    "inline-flex min-h-10 shrink-0 items-center gap-2 rounded-control px-2",
    "text-sm font-semibold transition-colors duration-(--motion-routine) ease-out",
    state.tone,
    className,
  );
  const label = `Pull request #${number}, ${status}`;
  const content = (
    <>
      <GitPullRequestIcon className="size-4" weight="bold" aria-hidden="true" />
      <span className="font-mono">#{number}</span>
      <span>{state.word}</span>
    </>
  );
  if (!url) {
    return (
      <span title={label} className={classes}>
        {content}
      </span>
    );
  }
  return (
    <a
      href={url}
      target="_blank"
      rel="noopener noreferrer"
      aria-label={label}
      onClick={(e) => e.stopPropagation()}
      className={classes}
    >
      {content}
    </a>
  );
}
