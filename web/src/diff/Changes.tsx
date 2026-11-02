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
import { Loader } from "../ui/components/Loader";
import { plural } from "../ui/lib/format";
import { Notice } from "../ui/page/Notice";
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
  if (query.isPending) return <Loader message="Reading the changes" />;
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
    <Suspense fallback={<Loader message="Reading the changes" />}>
      <DiffView diff={query.data.diff} truncated={query.data.truncated} />
    </Suspense>
  );
}

/**
 * The headline of a change's diff on a card: how many files it touches and what they cost. The card opens the whole
 * diff in a sheet, because a dock a hand's width wide is not where a diff is read. The sheet is as tall as the diff it
 * holds, up to the window less its margins, so a change of two files does not sit in a band of nothing; a big one
 * still gets the height of the screen and scrolls its own two columns. It closes on Escape or on its own X.
 */
export function ChangesCard({ of }: { of: { change: number } }) {
  const [open, setOpen] = useState(false);
  const query = useDiff(of);
  const headline = useMemo(() => {
    const diff = query.data?.diff;
    if (!diff) return null;
    const files = parseUnifiedDiff(diff);
    return { files: files.length, ...totals(files) };
  }, [query.data]);
  if (query.isPending) return <Loader message="Reading the changes" />;
  if (query.isError || headline === null) {
    return (
      <DiffReadFailed
        error={query.error}
        busy={query.isFetching}
        onRetry={() => void query.refetch()}
      />
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
        <span className="min-w-0 flex-1 text-sm text-secondary tabular-nums">
          {`${plural(headline.files, "file", "files")} · `}
          <Delta additions={headline.additions} deletions={headline.deletions} />
        </span>
        <ArrowsOutIcon className="size-4 shrink-0 text-secondary" aria-hidden="true" />
      </button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent data-testid="change-diff" aria-describedby={undefined} className="max-w-6xl">
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
