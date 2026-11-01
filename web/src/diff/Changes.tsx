/**
 * What a run or a change changed, read from the server (`GET /v1/runs/{id}/diff`, `GET /v1/changes/{id}/diff`) and
 * drawn as a diff. The events that move a change or end a run make it stale (`live/invalidation.ts`).
 */
import { useQuery } from "@tanstack/react-query";
import { lazy, Suspense } from "react";
import { changeDiff, runDiff } from "../api/calls/runs";
import { keys } from "../api/keys";
import { Loader } from "../ui/components/Loader";

// The diff view is loaded when a diff is first shown.
const DiffView = lazy(() => import("./DiffView").then((m) => ({ default: m.DiffView })));

export function Changes({ of }: { of: { run: number } | { change: number } }) {
  const isRun = "run" in of;
  const id = isRun ? of.run : of.change;
  const query = useQuery({
    queryKey: isRun ? keys.runDiff(id) : keys.changeDiff(id),
    queryFn: () => (isRun ? runDiff(id) : changeDiff(id)),
  });
  if (query.isPending) return <Loader message="Reading the changes…" />;
  if (query.isError) {
    return (
      <p role="alert" className="m-0 text-xs text-error">
        {query.error instanceof Error ? query.error.message : "cannot read the changes"}
      </p>
    );
  }
  return (
    <Suspense fallback={<Loader message="Reading the changes…" />}>
      <DiffView diff={query.data.diff} truncated={query.data.truncated} />
    </Suspense>
  );
}
