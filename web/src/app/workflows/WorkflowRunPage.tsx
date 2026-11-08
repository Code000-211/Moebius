/**
 * One run of a workflow: its status and trigger at the top, then a panel for each node with the input it was given, the
 * output it answered, its log and its error. Input, output and what a waiting node asks are shown as JSON, a redacted
 * value keeping the redaction mark the server sent, and a value too large for its panel is cut short until "Show all"
 * is pressed.
 *
 * A waiting run says on its own panel where it stopped and what it asks, and marks that node's panel, so a person can
 * see what a run needs without reading every node. The page reads `GET /v1/workflow-runs/{id}` and the run and node
 * events make it read again, so a node that starts, ends or asks something arrives without a reload.
 */
import { ArrowLeftIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { Link, useParams } from "react-router";
import { keys } from "../../api/keys";
import { type WorkflowNodeRun, workflowRun } from "../../api/workflows";
import { Tag } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import { elapsed } from "../../ui/lib/format";
import { JsonValue } from "../../ui/page/JsonValue";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { needsAttention, runStatus, triggerName } from "./model";

export function WorkflowRunPage() {
  const id = Number(useParams().id);
  const run = useQuery({
    queryKey: keys.workflowRun(id),
    queryFn: () => workflowRun(id),
    enabled: Number.isInteger(id) && id > 0,
  });
  const back = (
    <Button asChild variant="ghost" size="sm">
      <Link to="/workflows">
        <ArrowLeftIcon className="size-4" aria-hidden="true" />
        Workflows
      </Link>
    </Button>
  );

  if (!Number.isInteger(id) || id <= 0) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        <PageHeader title="Workflow run" back={back} />
        <ReadFailed
          error={new Error("This address does not name a run.")}
          fallback="No run at this address."
          onRetry={() => void run.refetch()}
        />
      </div>
    );
  }
  if (run.isPending) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        <PageHeader title={`Run ${id}`} back={back} />
        <SkeletonLines lines={8} message="Reading the run" />
      </div>
    );
  }
  if (run.isError) {
    return (
      <div className="flex h-full min-h-0 flex-col">
        <PageHeader title={`Run ${id}`} back={back} />
        <ReadFailed
          error={run.error}
          fallback="Could not read this run. Try again."
          busy={run.isFetching}
          onRetry={() => void run.refetch()}
        />
      </div>
    );
  }

  const shown = run.data;
  const status = runStatus(shown.status);
  const waiting = shown.waiting;
  const length =
    shown.started_at !== null && shown.finished_at !== null
      ? elapsed(shown.finished_at - shown.started_at)
      : null;
  return (
    <div className="flex h-full min-h-0 flex-col">
      <PageHeader
        title={`${shown.workflow} · run ${shown.id}`}
        back={back}
        context={`Started by ${triggerName(shown.trigger).toLowerCase()}${
          length === null ? "" : ` · took ${length}`
        }`}
        actions={
          <Link
            className="rounded-round text-sm underline-offset-4 hover:underline"
            to={`/workflows/${encodeURIComponent(shown.workflow)}`}
          >
            Open definition
          </Link>
        }
      />
      <div className="min-h-0 overflow-y-auto">
        <div className="flex flex-col gap-4 pb-6">
          <section
            aria-label="Run"
            className="flex flex-wrap items-center gap-3 rounded-card bg-surface p-4 shadow-card"
          >
            <Tag tone={needsAttention(shown.status) ? status.tone : "plain"}>{status.label}</Tag>
            <span className="text-sm text-secondary">{triggerName(shown.trigger)}</span>
            {shown.error ? (
              <p role="alert" className="m-0 w-full text-sm text-error-text">
                {shown.error}
              </p>
            ) : null}
          </section>

          {waiting ? <WaitingPanel node={waiting.node} asks={waiting.asks} /> : null}

          {shown.nodes.length === 0 ? (
            <p className="m-0 text-sm text-secondary">This run has no node yet.</p>
          ) : (
            <div className="flex flex-col gap-4">
              {shown.nodes.map((node) => (
                <NodePanel
                  key={`${node.node}-${node.attempt}`}
                  node={node}
                  waiting={waiting?.node === node.node}
                />
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

/** Where a waiting run stopped and what it asks, said before the node panels so it is not read past. */
function WaitingPanel({ node, asks }: { node: string; asks: unknown }) {
  return (
    <section
      aria-label="Waiting"
      className="flex flex-col gap-3 rounded-card bg-attention-fill p-4 text-attention-text shadow-card"
    >
      <h2 className="m-0 text-base font-semibold">Waiting at {node}</h2>
      <p className="m-0 text-sm">This run cannot go on until what it asks is answered.</p>
      <JsonValue value={asks} name="waiting-asks" />
    </section>
  );
}

/** One node of the run: what it was given, what it answered, what it logged and why it failed. */
function NodePanel({ node, waiting }: { node: WorkflowNodeRun; waiting: boolean }) {
  const status = runStatus(node.status);
  const length =
    node.started_at !== null && node.finished_at !== null
      ? elapsed(node.finished_at - node.started_at)
      : null;
  return (
    <section
      aria-label={`Node ${node.node}`}
      aria-current={waiting ? "step" : undefined}
      className={
        waiting
          ? "flex flex-col gap-4 rounded-card bg-surface p-4 shadow-card"
          : "flex flex-col gap-4 rounded-card bg-surface p-4 shadow-card"
      }
    >
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <h2 className="m-0 text-base font-semibold text-text">
          {node.node} <span className="font-normal text-secondary">· {node.kind}</span>
        </h2>
        <div className="flex flex-wrap items-center gap-2 text-sm text-secondary">
          <Tag tone={needsAttention(node.status) ? status.tone : "plain"}>{status.label}</Tag>
          <span>attempt {node.attempt}</span>
          {length === null ? null : <span className="tabular-nums">took {length}</span>}
        </div>
      </div>

      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        <Value title="Input" value={node.input} name={`${node.node}-input`} />
        <Value title="Output" value={node.output} name={`${node.node}-output`} />
      </div>

      <div className="flex flex-col gap-2">
        <h3 className="m-0 text-sm font-semibold text-text">Log</h3>
        {node.log ? (
          <pre className="m-0 max-h-80 overflow-auto whitespace-pre-wrap break-words rounded-control bg-code px-3 py-2 font-mono text-xs leading-5 text-text">
            {node.log}
          </pre>
        ) : (
          <p className="m-0 text-sm text-secondary">This node logged nothing.</p>
        )}
      </div>

      {node.error ? (
        <div role="alert" className="flex flex-col gap-1 rounded-control bg-error-fill p-3">
          <h3 className="m-0 text-sm font-semibold text-error-text">Error</h3>
          <p className="m-0 whitespace-pre-wrap break-words text-sm text-error-text">
            {node.error}
          </p>
        </div>
      ) : null}
    </section>
  );
}

function Value({ title, value, name }: { title: string; value: unknown; name: string }) {
  return (
    <div className="flex min-w-0 flex-col gap-2">
      <h3 className="m-0 text-sm font-semibold text-text">{title}</h3>
      <JsonValue value={value} name={name} />
    </div>
  );
}
