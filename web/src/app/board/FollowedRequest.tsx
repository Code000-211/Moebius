/**
 * What the card window says of a card that is someone else's work (ADR 0027), in the parts the other cards use: for a pull
 * request someone on another server opened, the pull request's badge, the avatar and login of whoever opened it and its
 * checks as an own change's are worded; for an issue someone took, one quiet sentence; and, when the server could not
 * assign its own person on the forge, why. The actions for the pull request (review it, merge it) are where every
 * change's are, in the block that asks for a person's turn (`NeedsYou.tsx`).
 */
import type { Board, Card } from "../../api/types/board";
import { Button } from "../../ui/components/Button";
import { PrBadge } from "../../ui/components/PrBadge";
import { Avatar } from "../../ui/page/Avatar";
import { Notice } from "../../ui/page/Notice";
import { CHECKS } from "./CardProperties";
import { changeOf, forgeName, pullRequestOf } from "./model";

export function FollowedRequest({
  board,
  card,
  busy = false,
  onRelease,
}: {
  board: Board;
  card: Card;
  busy?: boolean;
  /** The owner takes the assignee off an issue that was taken with `/take`. */
  onRelease?: () => void;
}) {
  const author = card.change?.outside_author;
  const change = author ? changeOf(board, card) : undefined;
  const pullRequest = pullRequestOf(board.repository, change);
  const forge = forgeName(board.repository) ?? "the forge";
  if (!author && !card.taken_by && !card.assign_problem) return null;
  return (
    <div className="flex min-w-0 flex-col gap-3 px-6 pb-4" data-testid="elsewhere">
      {author && (
        <div className="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-1">
          {pullRequest && (
            <PrBadge
              number={pullRequest.number}
              url={pullRequest.url}
              status={pullRequest.status}
            />
          )}
          <span className="inline-flex min-h-7 items-center gap-1.5 text-sm text-secondary">
            <Avatar handle={author} />
            {`by @${author}`}
          </span>
          {card.change && (
            <span className="text-sm text-secondary">
              {`Checks: ${CHECKS[card.change.checks] ?? card.change.checks}`}
            </span>
          )}
        </div>
      )}
      {!author && card.taken_by && card.taken_by_command && (
        <div className="flex min-w-0 flex-wrap items-center justify-between gap-x-4 gap-y-2">
          <p data-testid="taken-note" className="m-0 min-w-0 text-sm text-secondary">
            {`Taken by @${card.taken_by} with /take on ${forge}. This server leaves the issue alone until they release it, or you do.`}
          </p>
          {onRelease && (
            <Button size="sm" variant="secondary" disabled={busy} onClick={onRelease}>
              {`Release from @${card.taken_by}`}
            </Button>
          )}
        </div>
      )}
      {!author && card.taken_by && !card.taken_by_command && (
        <p data-testid="taken-note" className="m-0 text-sm text-secondary">
          {`Taken by @${card.taken_by} on ${forge}. This server leaves the issue alone until they are no longer its assignee there.`}
        </p>
      )}
      {card.assign_problem && <Notice tone="error">{card.assign_problem}</Notice>}
    </div>
  );
}
