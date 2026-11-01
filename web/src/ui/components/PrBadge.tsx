// From BloopAI/vibe-kanban packages/ui/src/components/PrBadge.tsx (Apache-2.0), changed: the url is optional (GitLab and Forgejo pull requests have no address the app knows), then the badge is not a link; the colours are Deep Velvet's (merged is the live purple).
import { GitPullRequestIcon } from "@phosphor-icons/react";
import { cn } from "../lib/cn";

export type PrBadgeStatus = "open" | "merged" | "closed";

export interface PrBadgeProps {
  number: number;
  url?: string | null;
  status: PrBadgeStatus;
  className?: string;
}

export function PrBadge({ number, url, status, className }: PrBadgeProps) {
  const classes = cn(
    "flex items-center gap-half px-1.5 py-0.5 rounded text-xs font-medium transition-colors",
    status === "merged"
      ? "bg-live-fill text-live hover:bg-live-fill/70"
      : status === "closed"
        ? "bg-red-fill text-error hover:bg-red-fill/70"
        : "bg-teal-fill text-success hover:bg-teal-fill/70",
    className,
  );
  const label = `Pull request #${number}, ${status}`;
  const content = (
    <>
      <GitPullRequestIcon className="size-icon-2xs" weight="bold" />
      <span>#{number}</span>
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
