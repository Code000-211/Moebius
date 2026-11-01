// From BloopAI/vibe-kanban packages/ui/src/components/KanbanCardContent.tsx (Apache-2.0), changed: no i18n; the card shows what a Moebius card has (its number as a link to the issue, the title wrapped, the line of the agent and round, a round bar, badges for its state and the pull request) in place of priority, assignees, tags, relationships and the description preview; the "more actions" button is gone (the dock has the actions).
import { cn } from "../lib/cn";
import { KanbanBadge, type KanbanBadgeProps } from "./KanbanBadge";
import { PrBadge, type PrBadgeStatus } from "./PrBadge";
import { RunningDots } from "./RunningDots";

export interface KanbanPullRequest {
  number: number;
  url?: string | null;
  status: PrBadgeStatus;
}

export type KanbanCardContentProps = {
  /** The card's number as it is written, such as `#12`. */
  displayId: string;
  /** Where the card's issue is, when the app knows; the number is then a link. */
  issueUrl?: string | null;
  title: string;
  /** The small line above the title: the agent and the round. */
  detail?: string | null;
  badges?: KanbanBadgeProps[];
  pullRequest?: KanbanPullRequest | null;
  /** The rounds so far of the most the card gets; shown as a bar when both are above zero. */
  rounds?: { done: number; limit: number } | null;
  /** The card's run is working: three dots run beside the number. */
  isLoading?: boolean;
  /** A card in Done is drawn quiet. */
  muted?: boolean;
  className?: string;
};

export function KanbanCardContent({
  displayId,
  issueUrl,
  title,
  detail,
  badges = [],
  pullRequest,
  rounds,
  isLoading = false,
  muted = false,
  className,
}: KanbanCardContentProps) {
  return (
    <div className={cn("flex min-w-0 flex-col gap-half", className)}>
      {/* Row 1: the number, the dots of a run that works, and the pull request */}
      <div className="flex items-center justify-between gap-half">
        <div className="flex min-w-0 items-center gap-base">
          {issueUrl ? (
            <a
              href={issueUrl}
              target="_blank"
              rel="noopener noreferrer"
              aria-label={`Issue ${displayId} on its forge`}
              onClick={(e) => e.stopPropagation()}
              className="truncate font-mono text-xs text-low hover:text-accent hover:underline"
            >
              {displayId}
            </a>
          ) : (
            <span className="truncate font-mono text-xs text-low">{displayId}</span>
          )}
          {isLoading && <RunningDots />}
        </div>
        {pullRequest && (
          <PrBadge number={pullRequest.number} url={pullRequest.url} status={pullRequest.status} />
        )}
      </div>

      {/* Row 2: the agent and the round */}
      {detail && <div className="truncate font-mono text-[11px] text-low">{detail}</div>}

      {/* Row 3: the title, wrapped, however long */}
      <span
        className={cn(
          "text-[13px] font-semibold [overflow-wrap:anywhere]",
          muted ? "text-low" : "text-high",
        )}
      >
        {title}
      </span>

      {/* Row 4: the rounds of the card, one segment each */}
      {rounds && rounds.limit > 0 && rounds.done > 0 && (
        <div
          role="img"
          aria-label={`Round ${rounds.done} of ${rounds.limit}`}
          className="flex gap-half"
        >
          {Array.from({ length: rounds.limit }, (_, round) => (
            <span
              // biome-ignore lint/suspicious/noArrayIndexKey: the segments are the rounds, in order
              key={round}
              className={cn(
                "h-[3px] flex-1 rounded-[2px]",
                round < rounds.done ? "bg-live" : "bg-track",
              )}
            />
          ))}
        </div>
      )}

      {/* Row 5: badges (own row, so that they wrap instead of overflowing) */}
      {badges.length > 0 && (
        <div className="flex min-w-0 flex-wrap items-center gap-half">
          {badges.map((badge) => (
            <KanbanBadge key={badge.name} {...badge} />
          ))}
        </div>
      )}
    </div>
  );
}
