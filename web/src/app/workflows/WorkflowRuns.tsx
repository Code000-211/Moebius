/**
 * The runs of one workflow, newest first, each a 36px row with its status, a link to its page, its age and, while it
 * can still be stopped, Cancel. The Runs tab shows all of them and the Overview its last few (`limit`), with the way
 * to the tab. It reads `GET /v1/projects/{id}/workflows/{name}/runs`, and the `workflow_*` events keep it current.
 */
import { StopIcon } from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router";
import { cancelWorkflowRun, projectWorkflowRuns } from "../../api/calls/workflows";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import { Tag } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import { ago } from "../../ui/lib/format";
import { useNow } from "../../ui/lib/useNow";
import { Notice } from "../../ui/page/Notice";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { workflowRunPath } from "../pages";
import { runStatus } from "./model";

export function WorkflowRuns({
  project,
  name,
  limit,
}: {
  project: number;
  name: string;
  /** Shows only this many of the newest runs; all of them when it is left out. */
  limit?: number;
}) {
  const queryClient = useQueryClient();
  const now = useNow();
  const runs = useQuery({
    queryKey: keys.projectWorkflowRuns(project, name),
    queryFn: () => projectWorkflowRuns(project, name),
  });
  const cancel = useMutation({
    mutationFn: (id: number) => cancelWorkflowRun(id),
    onSettled: async () => {
      await queryClient.invalidateQueries({ queryKey: keys.allWorkflowRuns });
      await queryClient.invalidateQueries({ queryKey: keys.workflows });
    },
  });
  if (runs.isError) {
    return (
      <Notice tone="error">{sayError(runs.error, "Could not read the runs. Try again.")}</Notice>
    );
  }
  if (runs.isPending) return <SkeletonLines lines={3} message="Reading the runs" />;
  const history = limit === undefined ? runs.data : runs.data.slice(0, limit);
  if (history.length === 0) {
    return <p className="m-0 text-sm text-secondary">This workflow has not run.</p>;
  }
  return (
    <>
      {/* biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with no marker from the accessibility tree entirely */}
      <ul role="list" className="m-0 list-none divide-y divide-hairline p-0">
        {history.map((each) => {
          const status = runStatus(each.status);
          const open =
            each.status === "queued" || each.status === "running" || each.status === "waiting";
          return (
            <li
              key={each.id}
              data-testid="workflow-run-row"
              className="flex h-row-list items-center gap-2.5 text-sm"
            >
              <Tag tone={status.tone}>{status.label}</Tag>
              <Link
                className="rounded-round underline-offset-4 hover:underline"
                to={workflowRunPath(each.id)}
              >
                Run {each.id}
              </Link>
              <span className="tabular-nums text-secondary">{ago(now, each.created_at)}</span>
              {open ? (
                <Button
                  variant="ghost"
                  size="sm"
                  className="ml-auto"
                  aria-label={`Cancel run ${each.id}`}
                  disabled={cancel.isPending}
                  onClick={() => cancel.mutate(each.id)}
                >
                  <StopIcon className="size-4" aria-hidden="true" />
                  Cancel
                </Button>
              ) : null}
            </li>
          );
        })}
      </ul>
      {cancel.isError ? (
        <Notice tone="error">
          {sayError(cancel.error, "Could not cancel the run. Try again.")}
        </Notice>
      ) : null}
    </>
  );
}
