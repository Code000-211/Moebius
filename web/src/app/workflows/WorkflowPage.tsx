/**
 * One workflow's definition: the graph saved in its file, read-only and at the positions the canvas saved, each node
 * with its kind and parameters, and the raw file beside it for copying. What starts it, how many nodes it has and how
 * its last run went are its properties, in the page's right column. The file is shown exactly as it is stored, so
 * what a person copies is what the harness configuration will read back.
 *
 * It reads `GET /v1/workflows/{name}`. A definition that could not be read says so and offers to read it again; a
 * workflow that does not exist is said as a missing workflow rather than as an empty graph. Delete is
 * `DELETE /v1/workflows/{name}`, asked twice: the actions open a dialog, which warns that a workflow shaped like an
 * automation disappears from the Automations page too, and a run queued or running refuses the deletion in the dialog.
 */
import {
  ArrowLeftIcon,
  PencilSimpleIcon,
  PlayIcon,
  StopIcon,
  TrashIcon,
} from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import {
  cancelWorkflowRun,
  deleteWorkflow,
  workflow,
  workflowRuns,
} from "../../api/calls/workflows";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import { CodeBlock } from "../../ui/chat/CodeBlock";
import { Tag } from "../../ui/chat/Tag";

import { Button } from "../../ui/components/Button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../../ui/components/Dialog";
import { ago, plural } from "../../ui/lib/format";
import { useNow } from "../../ui/lib/useNow";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { PageProperties, Property } from "../../ui/page/Properties";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { lastRunLine, looksLikeAutomation, runStatus, triggerName } from "./model";
import { RunWorkflowDialog } from "./RunWorkflowDialog";
import { WorkflowGraph } from "./WorkflowGraph";

export function WorkflowPage() {
  const { name = "" } = useParams();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const now = useNow();
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const [removeRefusal, setRemoveRefusal] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const definition = useQuery({
    queryKey: keys.workflow(name),
    queryFn: () => workflow(name),
  });
  const runs = useQuery({
    queryKey: keys.workflowRuns(name),
    queryFn: () => workflowRuns(name),
  });
  const cancel = useMutation({
    mutationFn: (id: number) => cancelWorkflowRun(id),
    onSettled: async () => {
      await queryClient.invalidateQueries({ queryKey: keys.workflowRuns(name) });
      await queryClient.invalidateQueries({ queryKey: keys.workflows });
    },
  });
  const remove = useMutation({
    mutationFn: () => deleteWorkflow(definition.data?.name ?? name),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: keys.workflows });
      queryClient.removeQueries({ queryKey: keys.workflow(definition.data?.name ?? name) });
      queryClient.removeQueries({ queryKey: keys.workflowRuns(definition.data?.name ?? name) });
      navigate("/workflows");
    },
    onError: (error) => setRemoveRefusal(sayError(error, "Could not delete it. Try again.")),
  });
  const back = (
    <Button asChild variant="ghost" size="sm">
      <Link to="/workflows">
        <ArrowLeftIcon className="size-4" aria-hidden="true" />
        Workflows
      </Link>
    </Button>
  );

  if (definition.isPending) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        <PageHeader title={name} back={back} />
        <SkeletonLines lines={8} message="Reading the definition" />
      </div>
    );
  }
  if (definition.isError) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        <PageHeader title={name} back={back} />
        <ReadFailed
          error={definition.error}
          fallback="Could not read this workflow's definition. Try again."
          busy={definition.isFetching}
          onRetry={() => void definition.refetch()}
        />
      </div>
    );
  }

  const saved = definition.data.definition;
  const file = definition.data.file;
  const history = runs.data ?? [];
  const last = history[0] ?? null;
  const leave = () => {
    remove.reset();
    setRemoveRefusal(null);
    setConfirmingDelete(false);
  };
  return (
    <div className="flex h-full min-h-0 flex-col">
      <PageHeader
        title={saved.name}
        back={back}
        context={saved.description}
        actions={
          <div className="flex items-center gap-2">
            <Button asChild variant="secondary" size="sm">
              <Link to={`/workflows/${encodeURIComponent(saved.name)}/canvas`}>
                <PencilSimpleIcon className="size-4" aria-hidden="true" />
                Edit on the canvas
              </Link>
            </Button>
            <Button variant="secondary" size="sm" onClick={() => setRunning(true)}>
              <PlayIcon className="size-4" aria-hidden="true" />
              Run
            </Button>
            {last ? (
              <Button asChild variant="secondary" size="sm">
                <Link to={`/workflow-runs/${last.id}`}>Open last run</Link>
              </Button>
            ) : null}
            <Button
              variant="danger"
              size="sm"
              onClick={() => setConfirmingDelete(true)}
              disabled={confirmingDelete}
            >
              <TrashIcon className="size-4" aria-hidden="true" />
              Delete workflow
            </Button>
          </div>
        }
      />
      <RunWorkflowDialog name={saved.name} open={running} onOpenChange={setRunning} />
      {confirmingDelete && (
        <Dialog open onOpenChange={(open) => !open && leave()}>
          <DialogContent>
            <form
              className="flex flex-col gap-4"
              onSubmit={(event) => {
                event.preventDefault();
                if (!remove.isPending) remove.mutate();
              }}
            >
              <DialogHeader>
                <DialogTitle>{`Delete ${saved.name}?`}</DialogTitle>
                <DialogDescription>
                  The workflow's file is deleted and committed. Its past runs stay listed.
                  {looksLikeAutomation(saved) &&
                    " This workflow is an automation: a schedule that starts one chat. Deleting it stops the automation, and it leaves the Automations page."}
                </DialogDescription>
              </DialogHeader>
              {removeRefusal && <Notice tone="error">{removeRefusal}</Notice>}
              <DialogFooter>
                <Button type="button" variant="ghost" onClick={leave}>
                  Keep it
                </Button>
                <Button type="submit" variant="danger" disabled={remove.isPending}>
                  {remove.isPending ? "Deleting" : "Delete"}
                </Button>
              </DialogFooter>
            </form>
          </DialogContent>
        </Dialog>
      )}
      <div className="grid min-h-0 flex-1 grid-cols-1 gap-4 lg:grid-cols-[minmax(0,1fr)_22rem]">
        <WorkflowGraph definition={saved} />
        <PageProperties label="Definition">
          <Property name="Trigger">{triggerName(saved.trigger[0]?.kind)}</Property>
          <Property name="Nodes">{plural(saved.nodes.length, "node", "nodes")}</Property>
          <Property name="Last run">
            {last
              ? `${lastRunLine(last)}${last.started_at === null ? "" : ` · ${ago(now, last.started_at)}`}.`
              : "This workflow has not run."}
          </Property>
        </PageProperties>
        <aside className="flex min-h-0 flex-col gap-4 overflow-y-auto">
          <section
            aria-labelledby="workflow-file-title"
            className="flex min-h-0 flex-col border-t border-hairline pt-4"
          >
            <h2 id="workflow-file-title" className="m-0 text-base font-semibold text-text">
              File
            </h2>
            <p className="mb-1 mt-1 text-sm text-secondary">
              The workflow as it is saved, ready to copy.
            </p>
            <CodeBlock language="TOML" code={file} />
          </section>
          <section
            aria-labelledby="workflow-runs-title"
            className="flex min-h-0 flex-col gap-2 border-t border-hairline pt-4"
          >
            <h2 id="workflow-runs-title" className="m-0 text-base font-semibold text-text">
              Runs
            </h2>
            {runs.isError ? (
              <Notice tone="error">
                {sayError(runs.error, "Could not read the runs. Try again.")}
              </Notice>
            ) : history.length === 0 ? (
              <p className="m-0 text-sm text-secondary">This workflow has not run.</p>
            ) : (
              // biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with no marker from the accessibility tree entirely
              <ul role="list" className="m-0 list-none divide-y divide-hairline p-0">
                {history.map((each) => {
                  const status = runStatus(each.status);
                  const open =
                    each.status === "queued" ||
                    each.status === "running" ||
                    each.status === "waiting";
                  return (
                    <li
                      key={each.id}
                      data-testid="workflow-run-row"
                      className="flex h-row-list items-center gap-2.5 text-sm"
                    >
                      <Tag tone={status.tone}>{status.label}</Tag>
                      <Link
                        className="rounded-round underline-offset-4 hover:underline"
                        to={`/workflow-runs/${each.id}`}
                      >
                        Run {each.id}
                      </Link>
                      <span className="tabular-nums text-secondary">
                        {ago(now, each.created_at)}
                      </span>
                      {open ? (
                        <Button
                          variant="ghost"
                          size="sm"
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
            )}
            {cancel.isError ? (
              <Notice tone="error">
                {sayError(cancel.error, "Could not cancel the run. Try again.")}
              </Notice>
            ) : null}
          </section>
        </aside>
      </div>
    </div>
  );
}
