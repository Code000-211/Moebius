/**
 * Everything of a card the window keeps out of sight, in a window of its own that the card's menu opens ("Properties…"):
 * its properties, each edited in place (`CardProperties.tsx`), the changes with their pull request, and the script of
 * its column, read-only. The thread and its reply field are what the card window shows; this is where a person goes to
 * change a label, a date or a repository, or to read a diff and the script.
 */
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router";
import { cardRuns, script } from "../../api/calls/board";
import { isApiError } from "../../api/http";
import { keys } from "../../api/keys";
import type { Board, Card, InboxItem } from "../../api/types/board";
import { useWrapLines } from "../../chat/wrapping";
import { ChangesCard } from "../../diff/Changes";
import { Button } from "../../ui/components/Button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "../../ui/components/Dialog";
import { PrBadge } from "../../ui/components/PrBadge";
import { cn } from "../../ui/lib/cn";
import { Avatar } from "../../ui/page/Avatar";
import { Notice } from "../../ui/page/Notice";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { scriptPath } from "../pages";
import { CardProperties, CHECKS } from "./CardProperties";
import { changeOf, changesOfCard, changeWorktree, pullRequestOf } from "./model";

export function CardDetails({
  board,
  card,
  item,
  stopped,
  boards,
  onOpenCard,
  open,
  onOpenChange,
}: {
  board: Board;
  card: Card;
  item: InboxItem | undefined;
  stopped?: InboxItem;
  boards?: Board[];
  onOpenCard: (card: Card) => void;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const repository = board.repository;
  const wrap = useWrapLines();
  const runs = useQuery({
    queryKey: keys.cardRuns(card.id),
    queryFn: () => cardRuns(card.id),
    enabled: open,
  });
  // The card runs the scripts of the project whose board it is on.
  const source = useQuery({
    queryKey: keys.script(card.project_id, card.column),
    queryFn: () => script(card.project_id, card.column),
    enabled: open,
  });
  const worktree = changeWorktree(card, runs.data ?? []);
  const changes = changesOfCard(card);
  // A change of the card's other repositories is read from the board of that repository.
  const known = boards ?? [board];
  const pullRequestIn = (changeId: number, name: string | undefined) => {
    const where = known.find((b) => b.repository.name === name)?.repository ?? repository;
    const found = known.flatMap((b) => b.changes).find((c) => c.id === changeId);
    return pullRequestOf(
      where,
      found ?? (changeId === card.change?.id ? changeOf(board, card) : undefined),
    );
  };
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        aria-describedby={undefined}
        className="max-w-3xl"
        // Escape in a field of a property leaves the field first; the window stays until the second one.
        onEscapeKeyDown={(event) => {
          const target = event.target as HTMLElement;
          if (target.tagName === "INPUT" || target.tagName === "TEXTAREA") event.preventDefault();
        }}
      >
        <DialogHeader>
          <DialogTitle>{`Properties of #${card.number}`}</DialogTitle>
        </DialogHeader>
        <CardProperties
          board={board}
          card={card}
          item={item}
          stopped={stopped}
          boards={boards}
          onOpenCard={onOpenCard}
        />
        {changes.length > 0 && (
          <div className="flex flex-col gap-4">
            {changes.map((change, index) => {
              const pullRequest = pullRequestIn(change.id, change.repository);
              return (
                <div
                  key={change.id}
                  data-testid="card-change"
                  className="flex min-w-0 flex-col gap-2"
                >
                  {changes.length > 1 && (
                    <p className="m-0 flex flex-wrap items-baseline gap-x-2 text-sm text-text">
                      <span className="font-medium">{change.repository}</span>
                      <span className="text-secondary">
                        {`Checks: ${CHECKS[change.checks] ?? change.checks}`}
                      </span>
                    </p>
                  )}
                  {pullRequest && (
                    <div className="flex flex-wrap items-center gap-x-4">
                      <PrBadge
                        number={pullRequest.number}
                        url={pullRequest.url}
                        status={pullRequest.status}
                      />
                      {change.outside_author && (
                        <span className="inline-flex min-h-7 items-center gap-1.5 text-sm text-secondary">
                          <Avatar handle={change.outside_author} />
                          {`by @${change.outside_author}`}
                        </span>
                      )}
                    </div>
                  )}
                  <ChangesCard
                    of={{ change: change.id }}
                    worktree={index === 0 ? worktree : null}
                  />
                </div>
              );
            })}
          </div>
        )}
        <div className="flex min-w-0 flex-col gap-2 pb-2">
          {source.isPending && <SkeletonLines lines={1} message="Reading" />}
          {source.error && (
            <Notice tone="error">
              {isApiError(source.error) ? source.error.message : "Could not read this."}
            </Notice>
          )}
          {source.data && (
            <>
              <p className="m-0 text-sm text-secondary">
                {`Script of ${card.column} · ${source.data.path} · ${source.data.source === "project" ? "this project's own" : "built in"}`}
              </p>
              <pre
                data-testid="script"
                className={cn(
                  "m-0 max-h-48 overflow-auto rounded-control bg-code-bg p-4 font-mono text-sm text-code-text",
                  wrap && "whitespace-pre-wrap break-words",
                )}
              >
                {source.data.text}
              </pre>
              <Button asChild variant="secondary" size="sm" className="w-fit">
                <Link to={scriptPath(card.project_id, card.column, repository.id)}>
                  Open the script
                </Link>
              </Button>
            </>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
