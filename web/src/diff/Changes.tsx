/**
 * What a run or a change changed, read from the server (`GET /v1/runs/{id}/diff`, `GET /v1/changes/{id}/diff`) and
 * drawn as a diff. The events that move a change or end a run make it stale (`live/invalidation.ts`). A run page has
 * the room for a diff; the board's dock does not, so `ChangesCard` shows the headline of a change and opens the whole
 * diff in a sheet over the page, where it can be read at the height of the window.
 */
import { ArrowsOutIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { lazy, Suspense, useMemo, useState } from "react";
import { changeDiff, runDiff } from "../api/calls/runs";
import { keys } from "../api/keys";
import { Button } from "../ui/components/Button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "../ui/components/Dialog";
import { fileName, plural } from "../ui/lib/format";
import { FadedLine } from "../ui/page/FadedLine";
import { Notice } from "../ui/page/Notice";
import { SkeletonLines } from "../ui/page/Skeleton";
import { Delta } from "./Delta";
import { parseUnifiedDiff, totals } from "./parse";

// The diff view is loaded when a diff is first shown.
const DiffView = lazy(() => import("./DiffView").then((m) => ({ default: m.DiffView })));

/** Which diff to read: one run's, or the change a card offered. */
type DiffOf = { run: number } | { change: number };

function useDiff(of: DiffOf) {
  const isRun = "run" in of;
  const id = isRun ? of.run : of.change;
  return useQuery({
    queryKey: isRun ? keys.runDiff(id) : keys.changeDiff(id),
    queryFn: () => (isRun ? runDiff(id) : changeDiff(id)),
  });
}

/** A diff that could not be read, and the way to ask for it again. */
function DiffReadFailed({
  error,
  busy,
  onRetry,
}: {
  error: unknown;
  busy: boolean;
  onRetry: () => void;
}) {
  return (
    <Notice
      tone="error"
      action={
        <Button size="sm" variant="secondary" disabled={busy} onClick={onRetry}>
          Try again
        </Button>
      }
    >
      <span className="font-semibold">Couldn't read the changes. </span>
      {error instanceof Error ? error.message : "The server did not answer for this diff."}
    </Notice>
  );
}

export function Changes({ of }: { of: DiffOf }) {
  const query = useDiff(of);
  if (query.isPending) return <SkeletonLines lines={10} message="Reading the changes" />;
  if (query.isError) {
    return (
      <DiffReadFailed
        error={query.error}
        busy={query.isFetching}
        onRetry={() => void query.refetch()}
      />
    );
  }
  return (
    <Suspense fallback={<SkeletonLines lines={10} message="Reading the changes" />}>
      <DiffView diff={query.data.diff} truncated={query.data.truncated} />
    </Suspense>
  );
}

/**
 * The headline of a change's diff on a card: how many files it touches, what they cost, and — when the caller
 * names the run that made it — the worktree it was made in. The card opens the whole
 * diff in a sheet, because a dock a hand's width wide is not where a diff is read. The sheet is as tall as the diff it
 * holds, up to the window less its margins, so a change of two files does not sit in a band of nothing; a big one
 * still gets the height of the screen and scrolls its own two columns. It closes on Escape or on its own X.
 */
export function ChangesCard({
  of,
  worktree = null,
}: {
  of: { change: number };
  /** The worktree the change was made in, shown by its name; none when no run is named. */
  worktree?: string | null;
}) {
  const [open, setOpen] = useState(false);
  const query = useDiff(of);
  const headline = useMemo(() => {
    const diff = query.data?.diff;
    if (diff === undefined) return null;
    const files = parseUnifiedDiff(diff);
    return { files: files.length, ...totals(files) };
  }, [query.data]);
  if (query.isPending)
    return (
      <SkeletonLines
        lines={1}
        message="Reading the changes"
        className="min-h-10 justify-center rounded-control bg-sunken px-3"
      />
    );
  if (query.isError || headline === null) {
    return (
      <DiffReadFailed
        error={query.error}
        busy={query.isFetching}
        onRetry={() => void query.refetch()}
      />
    );
  }
  // An empty diff is an answer: nothing changed, or the branch is gone after the merge. There is nothing to open.
  if (headline.files === 0) {
    return (
      <p data-testid="diff-empty" className="m-0 text-sm text-secondary">
        No changes
      </p>
    );
  }
  return (
    <>
      <button
        type="button"
        data-testid="diff-card"
        onClick={() => setOpen(true)}
        className="flex min-h-10 w-full items-center gap-3 rounded-control bg-sunken px-3 text-left transition-colors duration-(--motion-routine) ease-out hover:bg-hover"
      >
        <span
          className={`text-sm text-secondary tabular-nums ${worktree ? "shrink-0" : "min-w-0 flex-1"}`}
        >
          {`${plural(headline.files, "file", "files")} · `}
          <Delta additions={headline.additions} deletions={headline.deletions} />
        </span>
        {worktree && (
          <FadedLine
            text={fileName(worktree)}
            title={worktree}
            className="flex-1 text-sm text-secondary"
          />
        )}
        <ArrowsOutIcon className="size-4 shrink-0 text-secondary" aria-hidden="true" />
      </button>
      <Dialog open={open} onOpenChange={setOpen}>
        {/* The sheet answers for the scroll of its two columns and nothing else, so it is clipped rather than
            scrolled: a sheet that scrolled as well slid both columns up out of sight and left a band of nothing
            where they were, which is what a person met when the wheel landed on the sheet instead of a column. */}
        <DialogContent
          data-testid="change-diff"
          aria-describedby={undefined}
          className="max-w-[min(90vw,1600px)] overflow-hidden"
        >
          <div className="flex min-h-0 flex-1 flex-col gap-4 p-6">
            <DialogHeader>
              <DialogTitle>Changes</DialogTitle>
            </DialogHeader>
            <Changes of={of} />
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
