/**
 * One workflow's definition: the graph saved in its file, read-only and at the positions the canvas saved, each node
 * with its kind and parameters, and the raw file beside it for copying. The file is shown exactly as it is stored, so
 * what a person copies is what the harness configuration will read back.
 *
 * It reads `GET /v1/workflows/{name}`. A definition that could not be read says so and offers to read it again; a
 * workflow that does not exist is said as a missing workflow rather than as an empty graph.
 */
import { ArrowLeftIcon, PencilSimpleIcon, PlayIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { Link, useParams } from "react-router";
import { keys } from "../../api/keys";
import { workflow } from "../../api/workflows";
import { CodeBlock } from "../../ui/chat/CodeBlock";
import { Tag } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import { ago, plural } from "../../ui/lib/format";
import { useNow } from "../../ui/lib/useNow";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { lastRunLine, triggerName } from "./model";
import { WorkflowGraph } from "./WorkflowGraph";

export function WorkflowPage() {
  const { name = "" } = useParams();
  const now = useNow();
  const definition = useQuery({
    queryKey: keys.workflow(name),
    queryFn: () => workflow(name),
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

  const saved = definition.data;
  const last = saved.last_run;
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
            {last ? (
              <Button asChild variant="secondary" size="sm">
                <Link to={`/workflow-runs/${last.id}`}>
                  <PlayIcon className="size-4" aria-hidden="true" />
                  Open last run
                </Link>
              </Button>
            ) : null}
          </div>
        }
      />
      <div className="grid min-h-0 flex-1 grid-cols-1 gap-4 lg:grid-cols-[minmax(0,1fr)_22rem]">
        <WorkflowGraph definition={saved} />
        <aside className="flex min-h-0 flex-col gap-4 overflow-y-auto">
          <section
            aria-label="Definition"
            className="flex flex-col gap-3 rounded-card bg-surface p-4 shadow-card"
          >
            <div className="flex flex-wrap items-center gap-2">
              <Tag tone="plain">{triggerName(saved.trigger)}</Tag>
              <span className="text-sm text-secondary">
                {plural(saved.node_count, "node", "nodes")}
              </span>
            </div>
            <p className="m-0 text-sm text-secondary">
              {last
                ? `${lastRunLine(last)}${last.started_at === null ? "" : ` · ${ago(now, last.started_at)}`}.`
                : "This workflow has not run."}
            </p>
          </section>
          <section
            aria-labelledby="workflow-file-title"
            className="flex min-h-0 flex-col rounded-card bg-surface p-4 shadow-card"
          >
            <h2 id="workflow-file-title" className="m-0 text-base font-semibold text-text">
              File
            </h2>
            <p className="mb-1 mt-1 text-sm text-secondary">
              The workflow as it is saved, ready to copy.
            </p>
            <CodeBlock language="TOML" code={saved.file}>
              {saved.file}
            </CodeBlock>
          </section>
        </aside>
      </div>
    </div>
  );
}
