// From BloopAI/vibe-kanban packages/ui/src/components/KanbanCardContent.tsx (Apache-2.0), changed: no i18n; the card
// shows what a Moebius card has — the state of its run and its change in words, its title, the agent and the round,
// the rounds it has left of the most it gets, and the links to its issue and its pull request — in place of priority,
// assignees, tags, relationships and the description preview; the "more actions" button is gone (the dock has the
// actions); both links are 40px targets and only the issue number is monospaced; the state words are the shared
// `Status` pill, and the card is opened by its title being a button, so the card itself stays a group the links sit in.

import { cn } from "../lib/cn";
import { FadedClamp } from "../page/FadedClamp";
import { Status, type Tone } from "../page/Status";
import { PrBadge, type PrBadgeStatus } from "./PrBadge";
import { RunningDots } from "./RunningDots";

export interface KanbanPullRequest {
  number: number;
  url?: string | null;
  status: PrBadgeStatus;
}

/** A word of a card and the state its colour stands for, in the names of the shared pill (`page/Status`). */
type CardLabel = { name: string; tone?: Tone };

export type KanbanCardContentProps = {
  /** The card's number as it is written, such as `#12`. */
  displayId: string;
  /** Where the card's issue is, when the app knows; the number is then a link. */
  issueUrl?: string | null;
  title: string;
  /** The first lines of the issue behind the card; the whole of it is in the dock the card opens. */
  preview?: string | null;
  /** The line under the title: the agent and how far its rounds are. */
  detail?: string | null;
  badges?: CardLabel[];
  pullRequest?: KanbanPullRequest | null;
  /** The rounds so far of the most the card gets; shown as a bar when both are above zero. */
  rounds?: { done: number; limit: number } | null;
  /** The card's run is working: the dots run beside its badges. */
  isLoading?: boolean;
  /** A card in Done is drawn quiet. */
  muted?: boolean;
  /**
   * Opens the card. With it the title is a button a keyboard and a screen reader can use; without it — on the drag
   * overlay, which only shows what is under the pointer — the title stays the text it is.
   */
  onOpen?: () => void;
  className?: string;
};

export function KanbanCardContent({
  displayId,
  issueUrl,
  title,
  preview,
  detail,
  badges = [],
  pullRequest,
  rounds,
  isLoading = false,
  muted = false,
  onOpen,
  className,
}: KanbanCardContentProps) {
  return (
    <div className={cn("flex min-w-0 flex-col gap-3", className)}>
      {/* Row 1: the state of the card, in words */}
      {(isLoading || badges.length > 0) && (
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          {isLoading && <RunningDots />}
          {badges.map((badge) => (
            <Status key={badge.name} tone={badge.tone}>
              {badge.name}
            </Status>
          ))}
        </div>
      )}

      {/* Row 2: the title, wrapped, however long, and the control that opens the card */}
      {onOpen ? (
        <button
          type="button"
          onClick={(e) => {
            // The card answers a click on itself as well; this one is the title's, and opens the card once.
            e.stopPropagation();
            onOpen();
          }}
          className={cn(
            "flex min-h-10 w-full appearance-none items-center rounded-control bg-transparent p-0 text-left text-base font-semibold [overflow-wrap:anywhere]",
            muted ? "text-secondary" : "text-text",
          )}
        >
          {title}
        </button>
      ) : (
        <span
          className={cn(
            "text-base font-semibold [overflow-wrap:anywhere]",
            muted ? "text-secondary" : "text-text",
          )}
        >
          {title}
        </span>
      )}

      {/* Row 3: what the issue asks for, three lines of it, the whole of it a click away */}
      {preview && preview.trim() !== "" && (
        <FadedClamp text={preview.trim()} className="text-sm text-secondary" />
      )}

      {/* Row 4: the agent and the round */}
      {detail && (
        <span className="text-sm text-secondary tabular-nums [overflow-wrap:anywhere]">
          {detail}
        </span>
      )}

      {/* Row 5: the rounds of the card, one segment each */}
      {rounds && rounds.limit > 0 && rounds.done > 0 && (
        <div
          role="img"
          aria-label={`Round ${rounds.done} of ${rounds.limit}`}
          className="flex gap-1"
        >
          {Array.from({ length: rounds.limit }, (_, round) => (
            <span
              // biome-ignore lint/suspicious/noArrayIndexKey: the segments are the rounds, in order
              key={round}
              className={cn(
                "h-1 flex-1 rounded-full",
                round < rounds.done ? "bg-progress" : "bg-track",
              )}
            />
          ))}
        </div>
      )}

      {/* Row 6: where the card lives outside Moebius, each link a 40px target */}
      {(issueUrl || pullRequest) && (
        <div className="-mx-2 flex min-w-0 flex-wrap items-center justify-between gap-2">
          {issueUrl ? (
            <a
              href={issueUrl}
              target="_blank"
              rel="noopener noreferrer"
              aria-label={`Issue ${displayId} on its forge`}
              onClick={(e) => e.stopPropagation()}
              className="inline-flex min-h-10 items-center rounded-control px-2 font-mono text-sm text-secondary transition-colors duration-(--motion-routine) ease-out hover:bg-hover hover:text-text"
            >
              {displayId}
            </a>
          ) : (
            <span className="px-2 font-mono text-sm text-secondary">{displayId}</span>
          )}
          {pullRequest && (
            <PrBadge
              number={pullRequest.number}
              url={pullRequest.url}
              status={pullRequest.status}
            />
          )}
        </div>
      )}
    </div>
  );
}
