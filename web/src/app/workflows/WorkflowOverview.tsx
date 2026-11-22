/**
 * The Overview tab of a workflow's page: what starts it and how its last run went as two key rows, the steps as a
 * numbered list in plain words, one row for each node in the order the graph runs them, and its last few runs under
 * them. A step is its id and kind, a line or two of what it is given (`steps.ts`, with an expression such as
 * `{{ input.summary }}` in the code font) and the name of its output. Nothing on it is JSON.
 */
import {
  ChatCircleIcon,
  CircleIcon,
  CodeIcon,
  GitBranchIcon,
  GitMergeIcon,
  GlobeIcon,
  HandPalmIcon,
  type Icon,
  PaperPlaneTiltIcon,
  RepeatIcon,
  RobotIcon,
} from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router";
import { projectWorkflowRuns } from "../../api/calls/workflows";
import { keys } from "../../api/keys";
import type { WorkflowDefinition, WorkflowSummary } from "../../api/types/workflows";
import { Tag } from "../../ui/chat/Tag";
import { ago, plural } from "../../ui/lib/format";
import { useNow } from "../../ui/lib/useNow";
import { Property } from "../../ui/page/Property";
import { until } from "../automations/model";
import { workflowPath, workflowRunPath } from "../pages";
import { HookAddress } from "./HookAddress";
import { runStatus, triggerName } from "./model";
import {
  kindTitle,
  outputName,
  type Piece,
  paramsInWords,
  stepSummary,
  stepsInOrder,
} from "./steps";
import { cronOf, firingAt } from "./triggerWords";
import { WorkflowRuns } from "./WorkflowRuns";

const ICONS: Record<string, Icon> = {
  agent: RobotIcon,
  chat: ChatCircleIcon,
  code: CodeIcon,
  condition: GitBranchIcon,
  for_each: RepeatIcon,
  http: GlobeIcon,
  approval: HandPalmIcon,
  publish: PaperPlaneTiltIcon,
  merge: GitMergeIcon,
};

/** A summary as prose, with its expressions in the code font. */
export function Summary({ pieces }: { pieces: readonly Piece[] }) {
  // A piece is keyed by where its text starts, which no other piece of the same summary shares.
  let at = 0;
  return (
    <>
      {pieces.map((piece) => {
        const text = "expr" in piece ? piece.expr : piece.text;
        const key = at;
        at += text.length;
        return "expr" in piece ? (
          <code
            key={key}
            className="rounded-round bg-hover px-1 py-0.5 font-mono text-xs text-text-secondary"
          >
            {piece.expr}
          </code>
        ) : (
          <span key={key}>{piece.text}</span>
        );
      })}
    </>
  );
}

export function WorkflowOverview({
  project,
  definition,
  summary,
}: {
  project: number;
  definition: WorkflowDefinition;
  /** The workflow's row of the project's list, which holds what the server works out: the next firing and the hook's address. */
  summary: WorkflowSummary | undefined;
}) {
  const now = useNow();
  const runs = useQuery({
    queryKey: keys.projectWorkflowRuns(project, definition.name),
    queryFn: () => projectWorkflowRuns(project, definition.name),
  });
  const last = runs.data?.[0] ?? null;
  const steps = stepsInOrder(definition);
  return (
    <div className="flex max-w-4xl flex-col gap-6">
      {definition.description === "" ? null : (
        <p className="m-0 text-sm text-secondary">{definition.description}</p>
      )}
      <section aria-label="Definition" className="flex flex-col gap-1">
        <Property name="Trigger">
          <TriggerLine definition={definition} summary={summary} now={now} />
        </Property>
        <Property name="Last run">
          {last ? (
            <span className="flex flex-wrap items-center gap-2">
              <Tag tone={runStatus(last.status).tone}>{runStatus(last.status).label}</Tag>
              {last.started_at === null ? null : (
                <span className="text-secondary">{ago(now, last.started_at)}</span>
              )}
              <Link
                className="rounded-round text-accent-text underline-offset-4 hover:underline"
                to={workflowRunPath(last.id)}
              >
                Open run {last.id}
              </Link>
            </span>
          ) : (
            "This workflow has not run."
          )}
        </Property>
      </section>
      <section aria-labelledby="workflow-steps-title" className="flex flex-col gap-2">
        <h2 id="workflow-steps-title" className="m-0 text-base font-semibold text-text">
          Steps
          <span className="ml-2 text-sm font-normal text-secondary">
            {plural(steps.length, "step", "steps")}
          </span>
        </h2>
        {steps.length === 0 ? (
          <p className="m-0 text-sm text-secondary">This workflow has no steps yet.</p>
        ) : (
          <ol className="m-0 list-none divide-y divide-hairline p-0" data-testid="workflow-steps">
            {steps.map((node, index) => {
              const StepIcon = ICONS[node.kind] ?? CircleIcon;
              const output = outputName(node);
              return (
                <li
                  key={node.id}
                  data-testid="workflow-step"
                  className="grid min-h-row-list grid-cols-[1.5rem_1rem_minmax(0,1fr)] items-start gap-x-3 py-2"
                >
                  <span className="pt-px text-right text-sm tabular-nums text-text-muted">
                    {index + 1}
                  </span>
                  <StepIcon className="mt-0.5 size-4 text-text-muted" aria-hidden="true" />
                  <div className="flex min-w-0 flex-col gap-0.5">
                    <div className="flex min-w-0 flex-wrap items-center gap-2">
                      <span className="text-sm font-medium text-text">{node.id}</span>
                      <Tag>{kindTitle(node)}</Tag>
                      {output ? (
                        <span className="text-xs text-secondary">
                          Output <code className="font-mono text-text-secondary">{output}</code>
                        </span>
                      ) : null}
                    </div>
                    <p className="m-0 line-clamp-2 break-words text-sm text-secondary">
                      <Summary pieces={stepSummary(node)} />
                    </p>
                  </div>
                </li>
              );
            })}
          </ol>
        )}
      </section>
      <section aria-labelledby="workflow-recent-title" className="flex flex-col gap-1">
        <div className="flex items-baseline gap-3">
          <h2 id="workflow-recent-title" className="m-0 text-base font-semibold text-text">
            Runs
          </h2>
          <Link
            className="rounded-round text-sm text-accent-text underline-offset-4 hover:underline"
            to={workflowPath(project, definition.name, "runs")}
          >
            All runs
          </Link>
        </div>
        <WorkflowRuns project={project} name={definition.name} limit={5} />
      </section>
    </div>
  );
}

/** What starts the workflow, in words, with the address a webhook answers on and when a schedule fires next. */
function TriggerLine({
  definition,
  summary,
  now,
}: {
  definition: WorkflowDefinition;
  summary: WorkflowSummary | undefined;
  now: number;
}) {
  const trigger = definition.trigger[0];
  const name = triggerName(trigger?.kind);
  if (trigger?.kind === "webhook") {
    const address = summary?.hook_address;
    return (
      <span className="flex min-w-0 flex-wrap items-center gap-x-2">
        <span>{name}, answers on</span>
        {address === undefined ? (
          <span className="text-secondary">an address the server has not given yet</span>
        ) : (
          <HookAddress workflow={definition.name} address={address} />
        )}
      </span>
    );
  }
  if (trigger?.kind === "schedule") {
    const cron = cronOf(trigger);
    const next = summary?.next_firing;
    return (
      <span className="flex flex-wrap items-center gap-x-2">
        <span>{name}</span>
        {cron ? (
          <code className="rounded-round bg-hover px-1.5 py-0.5 font-mono text-xs text-text-secondary">
            {cron}
          </code>
        ) : null}
        <span className="text-secondary">
          {next === undefined
            ? "· no next firing"
            : `· next ${firingAt(now, next)} (${until(now, next)})`}
          {summary?.last_fired === undefined ? "" : ` · last fired ${ago(now, summary.last_fired)}`}
        </span>
      </span>
    );
  }
  if (trigger?.kind === "manual") {
    return <span>{name}, started with Run</span>;
  }
  const params = paramsInWords(trigger?.params);
  return (
    <span>
      {name}
      {params.length > 0 ? (
        <span className="text-secondary">
          {" · "}
          <Summary pieces={params} />
        </span>
      ) : null}
    </span>
  );
}
