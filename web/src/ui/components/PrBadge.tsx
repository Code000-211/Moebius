// From BloopAI/vibe-kanban packages/ui/src/components/PrBadge.tsx (Apache-2.0), changed: the url is optional (GitLab
// and Forgejo pull requests have no address the app knows), then the badge is not a link; the state is a word beside
// the number, so it does not rest on colour; the state is a status icon and grey text, never a coloured pill; the link is a 28px target.
import { GitPullRequestIcon } from "@phosphor-icons/react";
import { cn } from "../lib/cn";
import { StatusIcon, type StatusState } from "../page/StatusIcon";

export type PrBadgeStatus = "open" | "merged" | "closed";

/** The word and the mark of a pull request: work going on, work that got through, work that ended without one. */
const STATES: Record<PrBadgeStatus, { word: string; mark: StatusState }> = {
  open: { word: "Open", mark: "inReview" },
  merged: { word: "Merged", mark: "done" },
  closed: { word: "Closed", mark: "notStarted" },
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
    "inline-flex min-h-7 shrink-0 items-center gap-1.5 text-sm text-secondary",
    className,
  );
  const label = `Pull request #${number}, ${status}`;
  const content = (
    <>
      <StatusIcon state={state.mark} decorative />
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
