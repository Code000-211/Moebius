/**
 * The Workflows page: every workflow the harness configuration holds, as one card each. A card says what the workflow
 * is called and what it is for, what starts it, how many nodes its graph has, and how its last run went, which opens
 * that run. The name opens the definition. A workflow that has never run says so rather than showing a blank where a
 * status should be.
 *
 * The list is read from `GET /v1/workflows` and kept current by the run events, so a run that ends while the page is
 * open changes the card's last run without a reload.
 */
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router";
import { keys } from "../../api/keys";
import { workflows } from "../../api/workflows";
import { Tag } from "../../ui/chat/Tag";
import { ago } from "../../ui/lib/format";
import { useNow } from "../../ui/lib/useNow";
import { Empty } from "../../ui/page/Empty";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonList } from "../../ui/page/Skeleton";
import { lastRunLine, needsAttention, runStatus, triggerName } from "./model";

export function WorkflowsPage() {
  const now = useNow();
  const list = useQuery({ queryKey: keys.workflows, queryFn: workflows });

  if (list.isPending) {
    return (
      <>
        <PageHeader title="Workflows" context="Graphs of nodes that run on their own." />
        <SkeletonList rows={3} message="Reading the workflows" />
      </>
    );
  }
  if (list.isError) {
    return (
      <>
        <PageHeader title="Workflows" context="Graphs of nodes that run on their own." />
        <ReadFailed
          error={list.error}
          fallback="Could not read the workflows. Try again."
          busy={list.isFetching}
          onRetry={() => void list.refetch()}
        />
      </>
    );
  }

  return (
    <div className="flex h-full min-h-0 flex-col">
      <PageHeader
        title="Workflows"
        context="Graphs of nodes that run on their own, what starts them, and how their last run went."
      />
      {list.data.length === 0 ? (
        <Empty
          title="No workflow yet"
          body="A workflow is a graph of nodes saved in the harness configuration. When one is added, it appears here with what starts it and how its last run went."
        />
      ) : (
        <div className="min-h-0 overflow-y-auto">
          {/* biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with no marker from the accessibility tree entirely */}
          <ul role="list" className="m-0 grid list-none grid-cols-1 gap-3 p-0 xl:grid-cols-2">
            {list.data.map((each) => {
              const last = each.last_run;
              const status = last ? runStatus(last.status) : null;
              return (
                <li key={each.name}>
                  <article className="flex h-full flex-col gap-3 rounded-card bg-surface p-5 shadow-card">
                    <div className="flex flex-wrap items-start justify-between gap-3">
                      <h2 className="m-0 min-w-0 text-lg font-semibold tracking-tight text-text">
                        <Link
                          className="rounded-round underline-offset-4 hover:underline"
                          to={`/workflows/${encodeURIComponent(each.name)}`}
                        >
                          {each.name}
                        </Link>
                      </h2>
                      <Tag tone="plain">{triggerName(each.trigger)}</Tag>
                    </div>
                    <p className="m-0 max-w-prose text-sm leading-relaxed text-secondary">
                      {each.description}
                    </p>
                    <div className="mt-auto flex flex-wrap items-center justify-between gap-3 text-sm text-secondary">
                      <span>
                        {each.node_count} {each.node_count === 1 ? "node" : "nodes"}
                      </span>
                      {last && status ? (
                        <Link
                          className="flex min-w-0 items-center gap-2 rounded-round underline-offset-4 hover:underline"
                          to={`/workflow-runs/${last.id}`}
                        >
                          <Tag tone={needsAttention(last.status) ? status.tone : "plain"}>
                            {status.label}
                          </Tag>
                          <span className="tabular-nums">
                            {lastRunLine(last)}
                            {last.started_at === null ? "" : ` · ${ago(now, last.started_at)}`}
                          </span>
                        </Link>
                      ) : (
                        <span className="text-secondary">{lastRunLine(null)}</span>
                      )}
                    </div>
                  </article>
                </li>
              );
            })}
          </ul>
        </div>
      )}
    </div>
  );
}
