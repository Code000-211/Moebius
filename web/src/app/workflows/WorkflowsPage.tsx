/**
 * The Workflows page: every workflow the harness configuration holds, as one row each. A row says what the workflow
 * is called and what it is for, what starts it, how many nodes its graph has, and how its last run went, which opens
 * that run. The name opens the definition. A workflow that has never run says so rather than showing a blank where a
 * status should be.
 *
 * The list is read from `GET /v1/workflows` and kept current by the run events, so a run that ends while the page is
 * open changes the card's last run without a reload. The "New workflow" button — in the header, and in the empty state
 * — opens the dialog that starts a canvas with a blank draft (`NewWorkflowDialog.tsx`).
 */

import { PlusIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Link } from "react-router";
import { workflows } from "../../api/calls/workflows";
import { keys } from "../../api/keys";
import { Tag } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import { FilterBar } from "../../ui/filters/FilterBar";
import { readFilters } from "../../ui/filters/model";
import { useQueryParams } from "../../ui/filters/useQueryParams";
import { ago, plural } from "../../ui/lib/format";
import { useNow } from "../../ui/lib/useNow";
import { Empty } from "../../ui/page/Empty";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonList } from "../../ui/page/Skeleton";
import { FILTER_KEYS, filtered, filterFields, lastRunLine, runStatus, triggerName } from "./model";
import { NewWorkflowDialog } from "./NewWorkflowDialog";

export function WorkflowsPage() {
  const now = useNow();
  const list = useQuery({ queryKey: keys.workflows, queryFn: workflows });
  const [creating, setCreating] = useState(false);
  const [params, setQuery] = useQueryParams();
  // One way to begin, offered wherever the page has room for it: the header's right, and the empty state.
  const newButton = (
    <Button variant="secondary" size="sm" onClick={() => setCreating(true)}>
      <PlusIcon className="size-4" aria-hidden="true" />
      New workflow
    </Button>
  );

  if (list.isPending) {
    return (
      <>
        <PageHeader title="Workflows" actions={newButton} />
        <SkeletonList rows={3} message="Reading the workflows" />
        <NewWorkflowDialog open={creating} onOpenChange={setCreating} />
      </>
    );
  }
  if (list.isError) {
    return (
      <>
        <PageHeader title="Workflows" actions={newButton} />
        <ReadFailed
          error={list.error}
          fallback="Could not read the workflows. Try again."
          busy={list.isFetching}
          onRetry={() => void list.refetch()}
        />
        <NewWorkflowDialog open={creating} onOpenChange={setCreating} />
      </>
    );
  }

  const everything = list.data;
  const fields = filterFields(
    everything,
    readFilters(
      params,
      FILTER_KEYS.map((key) => ({ key, label: key, options: [] })),
    ),
  );
  const values = readFilters(params, fields);
  const clear = () => setQuery(Object.fromEntries(FILTER_KEYS.map((key) => [key, ""])));
  const shown = filtered(everything, values);
  return (
    <div className="flex h-full min-h-0 flex-col">
      <PageHeader title="Workflows" actions={newButton} />
      {everything.length > 0 && (
        <FilterBar
          label="Filter the workflows"
          fields={fields}
          values={values}
          onFilter={(key, value) => setQuery({ [key]: value })}
          onClear={clear}
        />
      )}
      {everything.length === 0 ? (
        <Empty
          title="No workflow yet"
          body="A workflow is a graph of nodes saved in the harness configuration. When one is added, it appears here with what starts it and how its last run went."
          action={newButton}
        />
      ) : shown.length === 0 ? (
        <Empty
          title="No workflow matches"
          body="No workflow passes these filters. Clear them to see every workflow."
          action={
            <Button variant="secondary" onClick={clear}>
              Clear filters
            </Button>
          }
        />
      ) : (
        <div className="min-h-0 overflow-y-auto">
          {/* biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with no marker from the accessibility tree entirely */}
          <ul role="list" className="m-0 list-none divide-y divide-hairline p-0">
            {shown.map((each) => {
              const last = each.last_run;
              const status = last ? runStatus(last.status) : null;
              return (
                <li
                  key={each.name}
                  data-testid="workflow-row"
                  className="flex h-row-list items-center gap-x-2.5 px-4 hover:bg-hover"
                >
                  <span className="flex min-w-0 flex-1 items-baseline gap-2.5">
                    <h2 className="m-0 shrink-0 truncate text-base font-medium text-text">
                      <Link
                        className="rounded-round underline-offset-4 hover:underline"
                        to={`/workflows/${encodeURIComponent(each.name)}`}
                      >
                        {each.name}
                      </Link>
                    </h2>
                    <span className="min-w-0 truncate text-xs text-text-muted">
                      {each.description}
                    </span>
                  </span>
                  <span className="hidden w-28 shrink-0 text-xs text-text-muted sm:block">
                    {triggerName(each.trigger_kind)}
                  </span>
                  <span className="hidden w-16 shrink-0 text-xs tabular-nums text-text-muted sm:block">
                    {plural(each.node_count, "node", "nodes")}
                  </span>
                  {last && status ? (
                    <Link
                      className="flex w-72 min-w-0 shrink-0 items-center gap-2 rounded-round text-xs text-text-muted underline-offset-4 hover:underline"
                      to={`/workflow-runs/${last.id}`}
                    >
                      <Tag tone={status.tone}>{status.label}</Tag>
                      <span className="truncate tabular-nums">
                        {lastRunLine(last)}
                        {last.started_at === null ? "" : ` · ${ago(now, last.started_at)}`}
                      </span>
                    </Link>
                  ) : (
                    <span className="w-72 shrink-0 text-xs text-text-muted">
                      {lastRunLine(null)}
                    </span>
                  )}
                </li>
              );
            })}
          </ul>
        </div>
      )}
      <NewWorkflowDialog open={creating} onOpenChange={setCreating} />
    </div>
  );
}
