/**
 * The Workflows tab of a project: what the project's workflows are and what they have done, in one place (ADR 0025).
 * Above, the project's workflow definitions, one row each: the name, what it is for, what starts it and how its last
 * run went; the name opens the definition, which opens the canvas. Below, the runs of this project, newest first,
 * filtered All, Scheduled or Failed. The Scheduled filter first lists the project's schedule-trigger workflows with when
 * each fires next, and edits one in the editor window (`../automations/AutomationEditor.tsx`); "New scheduled
 * workflow" opens that window with the automation template. "Run a workflow here" at the far right of the header
 * starts a run of one of the project's manual workflows in one of the project's repositories
 * (`RunWorkflowDialog.tsx`), and "New workflow" opens a blank canvas.
 *
 * Only this project's workflows and runs are read (`GET /v1/projects/{id}/workflows`, `GET /v1/workflow-runs?project=`
 * and `GET /v1/automations`, of which the project's are kept). The run events keep the lists current. The chosen filter
 * (`?runs=`) and the open automation (`?automation=<id>` or `new`) are in the address, so a reload and a link keep them.
 */
import { PlayIcon, PlusIcon, TreeStructureIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { useId, useState } from "react";
import { Link, useSearchParams } from "react-router";
import { automations } from "../../api/calls/automations";
import { projectWorkflowRuns, projectWorkflows } from "../../api/calls/workflows";
import { keys } from "../../api/keys";
import type { Automation } from "../../api/types/automations";
import type { Project } from "../../api/types/projects";
import type { WorkflowRunSummary, WorkflowSummary } from "../../api/types/workflows";
import { Tag } from "../../ui/chat/Tag";
import { Button } from "../../ui/components/Button";
import { cn } from "../../ui/lib/cn";
import { ago } from "../../ui/lib/format";
import { useNow } from "../../ui/lib/useNow";
import { Empty } from "../../ui/page/Empty";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonList } from "../../ui/page/Skeleton";
import { TabPanel, Tabs } from "../../ui/page/Tabs";
import { Editor } from "../automations/AutomationEditor";
import {
  type Form,
  newForm,
  SUGGESTED_AUTOMATIONS,
  scheduleName,
  until,
} from "../automations/model";
import { workflowPath, workflowRunPath } from "../pages";
import { ProjectHeader } from "../projects/ProjectHeader";
import type { RunFilter } from "./model";
import { lastRunLine, runFilter, runStatus, scheduledWorkflows, triggerName } from "./model";
import { NewWorkflowDialog } from "./NewWorkflowDialog";
import { RunWorkflowDialog } from "./RunWorkflowDialog";

const NEW = "new";

const FILTERS: Array<{ value: RunFilter; label: string }> = [
  { value: "all", label: "All" },
  { value: "scheduled", label: "Scheduled" },
  { value: "failed", label: "Failed" },
];

/** The word where an automation is not simply on: Failed, Skipped or Off. */
function statusWord(automation: Automation): { text: string; className: string } | null {
  const last = automation.last_firing;
  if (last?.status === "failed") return { text: "Failed", className: "text-error-text" };
  if (last?.status === "skipped" && automation.enabled)
    return { text: "Skipped", className: "text-attention-text" };
  return automation.enabled ? null : { text: "Off", className: "text-muted" };
}

const ROW = "flex h-row-list items-center gap-x-2.5 px-4 hover:bg-hover";

export function WorkflowsTab({
  project,
  onTerminal,
}: {
  project: Project;
  onTerminal: () => void;
}) {
  const now = useNow();
  const base = useId();
  const [params, setParams] = useSearchParams();
  const [running, setRunning] = useState(false);
  const [creating, setCreating] = useState(false);
  /** The automation the form starts from, when the empty state offered one. */
  const [starting, setStarting] = useState<Form | null>(null);
  const definitions = useQuery({
    queryKey: keys.projectWorkflows(project.id),
    queryFn: () => projectWorkflows(project.id),
  });
  const runs = useQuery({
    queryKey: keys.projectWorkflowRuns(project.id),
    queryFn: () => projectWorkflowRuns(project.id),
  });
  const scheduled = useQuery({ queryKey: keys.automations, queryFn: automations });

  const filter = runFilter(params.get("runs"));
  const setFilter = (next: RunFilter) =>
    setParams(
      (current) => {
        const query = new URLSearchParams(current);
        if (next === "all") query.delete("runs");
        else query.set("runs", next);
        return query;
      },
      { replace: true },
    );
  const asked = params.get("automation");
  const chosen: number | typeof NEW | null =
    asked === NEW ? NEW : asked !== null && Number.isInteger(Number(asked)) ? Number(asked) : null;
  const setChosen = (next: number | typeof NEW | null) =>
    setParams(
      (current) => {
        const query = new URLSearchParams(current);
        if (next === null) query.delete("automation");
        else query.set("automation", String(next));
        return query;
      },
      { replace: true },
    );
  const startNew = (from: Form | null) => {
    setStarting(from);
    setChosen(NEW);
  };
  // Only this project's automations: the server lists them all.
  const mine = (scheduled.data ?? []).filter((each) => each.project_id === project.id);
  const editing = typeof chosen === "number" ? mine.find((each) => each.id === chosen) : undefined;

  const runButton = (
    <Button size="sm" onClick={() => setRunning(true)}>
      <PlayIcon className="size-4" weight="fill" aria-hidden="true" />
      Run a workflow here
    </Button>
  );
  const header = (
    <ProjectHeader project={project} tab="workflows" primary={runButton} onTerminal={onTerminal} />
  );
  const dialogs = (
    <>
      <RunWorkflowDialog
        project={project}
        workflows={definitions.data}
        open={running}
        onOpenChange={setRunning}
      />
      <NewWorkflowDialog project={project.id} open={creating} onOpenChange={setCreating} />
      {chosen === NEW || editing ? (
        <Editor
          key={editing?.id ?? NEW}
          project={project}
          saved={editing}
          starting={starting}
          now={now}
          onClose={() => setChosen(null)}
        />
      ) : null}
    </>
  );

  if (definitions.isPending || runs.isPending) {
    return (
      <>
        {header}
        <SkeletonList rows={4} message="Reading the workflows" />
        {dialogs}
      </>
    );
  }
  if (definitions.isError || runs.isError) {
    const failed = definitions.isError ? definitions : runs;
    return (
      <>
        {header}
        <ReadFailed
          error={failed.error}
          fallback="The workflows could not be read. Try again."
          busy={failed.isFetching}
          onRetry={() => void failed.refetch()}
        />
        {dialogs}
      </>
    );
  }

  const workflows = definitions.data;
  const repositoryName = (id: number): string =>
    project.repositories.find((each) => each.id === id)?.name ?? "project files";
  const shownRuns = runs.data.filter((each) =>
    filter === "failed"
      ? each.status === "failed"
      : filter === "scheduled"
        ? each.trigger === "schedule"
        : true,
  );
  const schedules = scheduledWorkflows(workflows, mine);
  const newScheduled = (
    <Button variant="secondary" size="sm" onClick={() => startNew(null)}>
      <PlusIcon className="size-4" aria-hidden="true" />
      New scheduled workflow
    </Button>
  );
  const newWorkflow = (
    <Button variant="secondary" size="sm" onClick={() => setCreating(true)}>
      <PlusIcon className="size-4" aria-hidden="true" />
      New workflow
    </Button>
  );

  return (
    <div className="flex h-full min-h-0 flex-col">
      {header}
      <div className="min-h-0 flex-1 overflow-y-auto">
        <section aria-labelledby={`${base}-definitions`} className="flex flex-col">
          <div className="flex items-center justify-between gap-2 px-4 pt-3 pb-1">
            <h2 id={`${base}-definitions`} className="m-0 text-base font-semibold text-text">
              Workflows
            </h2>
            <div className="flex items-center gap-2">
              {newScheduled}
              {newWorkflow}
            </div>
          </div>
          {workflows.length === 0 ? (
            <Empty
              title="No workflow yet"
              body="A workflow is a graph of nodes that this project owns. Start from a blank canvas, or from a schedule that starts a chat."
              action={
                <div className="flex flex-wrap gap-2">
                  {SUGGESTED_AUTOMATIONS.map((each) => (
                    <Button
                      key={each.name}
                      size="sm"
                      variant="secondary"
                      onClick={() => startNew({ ...newForm(), ...each })}
                    >
                      {each.name}
                    </Button>
                  ))}
                </div>
              }
              className="px-4"
            />
          ) : (
            <DefinitionList project={project.id} workflows={workflows} now={now} />
          )}
        </section>

        <section aria-labelledby={`${base}-runs`} className="mt-4 flex flex-col">
          <div className="flex items-center justify-between gap-2 border-b border-hairline px-4 pt-3 pb-px">
            <h2 id={`${base}-runs`} className="sr-only">
              Runs
            </h2>
            <Tabs tabs={FILTERS} value={filter} onChange={setFilter} label="Runs" idBase={base} />
          </div>
          <TabPanel idBase={base} value={filter}>
            {filter === "scheduled" && (
              <ScheduledList
                project={project.id}
                schedules={schedules}
                now={now}
                onEdit={setChosen}
              />
            )}
            {shownRuns.length === 0 ? (
              <p className="m-0 px-4 py-4 text-sm text-secondary">
                {filter === "failed"
                  ? "No run of this project has failed."
                  : filter === "scheduled"
                    ? "No scheduled run yet."
                    : "This project's workflows have not run yet."}
              </p>
            ) : (
              // biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with no marker from the accessibility tree entirely
              <ul role="list" className="m-0 list-none divide-y divide-hairline p-0">
                {shownRuns.map((each) => (
                  <RunRow
                    key={each.id}
                    run={each}
                    where={repositoryName(each.repository_id)}
                    now={now}
                  />
                ))}
              </ul>
            )}
          </TabPanel>
        </section>
      </div>
      {dialogs}
    </div>
  );
}

/** The project's workflow definitions: name and purpose, what starts it, its size and its last run. */
function DefinitionList({
  project,
  workflows,
  now,
}: {
  project: number;
  workflows: readonly WorkflowSummary[];
  now: number;
}) {
  return (
    // biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with no marker from the accessibility tree entirely
    <ul role="list" className="m-0 list-none divide-y divide-hairline p-0">
      {workflows.map((each) => {
        const last = each.last_run;
        const status = last ? runStatus(last.status) : null;
        return (
          <li key={each.name} data-testid="workflow-row" className={ROW}>
            <span className="flex min-w-0 flex-1 items-baseline gap-2.5">
              <TreeStructureIcon
                className="size-4 shrink-0 translate-y-0.5 text-text-muted"
                aria-hidden="true"
              />
              <h3 className="m-0 shrink-0 truncate text-base font-medium text-text">
                <Link
                  className="rounded-round underline-offset-4 hover:underline"
                  to={workflowPath(project, each.name)}
                >
                  {each.name}
                </Link>
              </h3>
              <span className="min-w-0 truncate text-xs text-text-muted">{each.description}</span>
            </span>
            <span className="hidden w-24 shrink-0 text-xs text-text-muted sm:block">
              {triggerName(each.trigger_kind)}
            </span>
            {last && status ? (
              <Link
                className="flex w-56 min-w-0 shrink-0 items-center gap-2 rounded-round text-xs text-text-muted underline-offset-4 hover:underline"
                to={workflowRunPath(last.id)}
              >
                <Tag tone={status.tone}>{status.label}</Tag>
                <span className="truncate tabular-nums">
                  {last.started_at === null ? "" : ago(now, last.started_at)}
                </span>
              </Link>
            ) : (
              <span className="w-56 shrink-0 text-xs text-text-muted">{lastRunLine(null)}</span>
            )}
          </li>
        );
      })}
    </ul>
  );
}

/** The project's schedule-trigger workflows with when each fires next; an automation's row edits it in the window. */
function ScheduledList({
  project,
  schedules,
  now,
  onEdit,
}: {
  project: number;
  schedules: ReturnType<typeof scheduledWorkflows>;
  now: number;
  onEdit: (id: number) => void;
}) {
  if (schedules.length === 0)
    return (
      <p className="m-0 px-4 py-4 text-sm text-secondary">
        No workflow of this project starts on a schedule. "New scheduled workflow" makes one.
      </p>
    );
  return (
    <div className="mb-2 border-b border-hairline">
      {/* biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with no marker from the accessibility tree entirely */}
      <ul role="list" className="m-0 list-none divide-y divide-hairline p-0">
        {schedules.map(({ workflow, automation }) => {
          const state = automation ? statusWord(automation) : null;
          return (
            <li key={workflow.name} data-testid="scheduled-row" className={ROW}>
              <TreeStructureIcon className="size-4 shrink-0 text-text-muted" aria-hidden="true" />
              <h3 className="m-0 shrink-0 truncate text-base font-medium text-text">
                <Link
                  className="rounded-round underline-offset-4 hover:underline"
                  to={workflowPath(project, workflow.name)}
                >
                  {automation?.name ?? workflow.name}
                </Link>
              </h3>
              <span
                className={cn(
                  "min-w-0 flex-1 truncate text-xs",
                  automation?.last_firing?.status === "failed"
                    ? "text-error-text"
                    : "text-text-muted",
                )}
              >
                {automation === undefined
                  ? "Schedule"
                  : automation.last_firing?.status === "failed"
                    ? automation.last_firing.reason
                    : automation.next_firing !== null
                      ? `${scheduleName(automation.schedule)} · next ${until(now, automation.next_firing)}`
                      : scheduleName(automation.schedule)}
              </span>
              {state && (
                <span className={cn("shrink-0 text-xs", state.className)}>{state.text}</span>
              )}
              {automation && (
                <Button
                  variant="ghost"
                  size="sm"
                  aria-label={`Edit scheduled workflow ${automation.name}`}
                  onClick={() => onEdit(automation.id)}
                >
                  Edit
                </Button>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

/** One run: its workflow, where it ran and how it stands, and when it started. */
function RunRow({ run, where, now }: { run: WorkflowRunSummary; where: string; now: number }) {
  const status = runStatus(run.status);
  const failed = run.status === "failed";
  return (
    <li data-testid="run-row">
      <Link to={workflowRunPath(run.id)} className={cn(ROW, "no-underline")}>
        <TreeStructureIcon className="size-4 shrink-0 text-text-muted" aria-hidden="true" />
        <span className="shrink-0 truncate text-base font-medium text-text">{run.workflow}</span>
        <span
          className={cn(
            "min-w-0 flex-1 truncate text-xs",
            failed ? "text-error-text" : "text-text-muted",
          )}
        >
          {where} · {status.label.toLowerCase()}
          {failed && run.error ? ` · ${run.error}` : ""}
        </span>
        <span className="shrink-0 text-xs tabular-nums text-text-muted">
          {ago(now, run.started_at ?? run.created_at)}
        </span>
      </Link>
    </li>
  );
}
