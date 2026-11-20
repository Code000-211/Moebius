/**
 * The rows of the Milestones page: every milestone is a row on one time axis, its bar running from the day it was made
 * to its target date and filled as far as its cards are done, as the cycles are on the Cycles page. A click on a row
 * opens what the milestone holds under it: its description, its share in each project and its cards.
 */
import { PencilSimpleIcon, TrashIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router";
import { readMilestone } from "../../api/calls/milestones";
import { keys } from "../../api/keys";
import type { MilestoneView } from "../../api/types/milestones";
import { Markdown } from "../../ui/chat/Markdown";
import { Button } from "../../ui/components/Button";
import { cn } from "../../ui/lib/cn";
import { axisOf, TimeBar } from "../../ui/page/TimeBar";
import { cardPath } from "../pages";
import { dateWords, overdue, percent, progressWords, projectsWords, spanOf } from "./model";

/** The milestones as rows on one time axis, in the order given. */
export function Timeline({
  list,
  now,
  opened,
  onToggle,
  onEdit,
  onDelete,
}: {
  list: MilestoneView[];
  now: string;
  /** The id of the milestone that is open, if any. */
  opened: number | null;
  onToggle: (milestone: MilestoneView) => void;
  onEdit: (milestone: MilestoneView) => void;
  onDelete: (milestone: MilestoneView) => void;
}) {
  const axis = axisOf(list.map(spanOf));
  if (axis === null) return null;
  return (
    <ul className="m-0 flex list-none flex-col p-0" aria-label="Milestones">
      {list.map((milestone) => (
        <Row
          key={milestone.id}
          milestone={milestone}
          now={now}
          axis={axis}
          open={milestone.id === opened}
          onToggle={() => onToggle(milestone)}
          onEdit={() => onEdit(milestone)}
          onDelete={() => onDelete(milestone)}
        />
      ))}
    </ul>
  );
}

/** One milestone: its name and projects, its bar on the axis, its date; once opened, its detail below. */
function Row({
  milestone,
  now,
  axis,
  open,
  onToggle,
  onEdit,
  onDelete,
}: {
  milestone: MilestoneView;
  now: string;
  axis: { start: string; end: string };
  open: boolean;
  onToggle: () => void;
  onEdit: () => void;
  onDelete: () => void;
}) {
  const late = overdue(milestone, now);
  const span = spanOf(milestone);
  return (
    <li className="border-b border-hairline" data-testid="milestone">
      <button
        type="button"
        aria-expanded={open}
        onClick={onToggle}
        className={cn(
          "grid min-h-9 w-full cursor-pointer appearance-none grid-cols-[minmax(0,1fr)_auto] items-center gap-x-4 gap-y-1 bg-transparent px-3 py-2 text-left hover:bg-row-hover",
          "sm:grid-cols-[minmax(0,12rem)_minmax(0,1fr)_9rem]",
        )}
      >
        <span className="flex min-w-0 items-baseline gap-2">
          <span className="min-w-0 truncate text-base font-medium text-text" title={milestone.name}>
            {milestone.name}
          </span>
          <span className="shrink-0 text-sm text-secondary">{projectsWords(milestone)}</span>
        </span>
        <span
          className={cn(
            "shrink-0 whitespace-nowrap text-right text-sm sm:col-start-3 sm:row-start-1",
            late ? "text-attention-text" : "text-secondary",
          )}
        >
          {late
            ? `Overdue · ${dateWords(milestone.target_date, now)}`
            : dateWords(milestone.target_date, now)}
        </span>
        <span className="col-span-2 flex items-center gap-3 sm:col-span-1 sm:col-start-2 sm:row-start-1">
          <TimeBar
            className="flex-1"
            start={span.start}
            end={span.end}
            axis={axis}
            progress={percent(milestone.done, milestone.cards)}
            label={progressWords(milestone.done, milestone.cards)}
            today={now}
          />
          <span className="shrink-0 text-sm text-secondary tabular-nums">
            {progressWords(milestone.done, milestone.cards)}
          </span>
        </span>
      </button>
      {open && <Detail milestone={milestone} onEdit={onEdit} onDelete={onDelete} />}
    </li>
  );
}

/** What a milestone holds: its description, its share in each project and its cards, read when it is opened. */
export function Detail({
  milestone,
  onEdit,
  onDelete,
}: {
  milestone: MilestoneView;
  onEdit: () => void;
  onDelete: () => void;
}) {
  const detail = useQuery({
    queryKey: keys.milestone(milestone.id),
    queryFn: () => readMilestone(milestone.id),
  });
  return (
    <div className="flex flex-col gap-3 px-3 pb-3 pt-1">
      <div className="flex gap-2">
        <Button size="sm" variant="secondary" onClick={onEdit}>
          <PencilSimpleIcon className="size-4" aria-hidden="true" />
          Edit
        </Button>
        <Button size="sm" variant="danger" onClick={onDelete}>
          <TrashIcon className="size-4" aria-hidden="true" />
          Delete
        </Button>
      </div>
      {milestone.description !== "" && (
        <Markdown content={milestone.description} className="max-w-prose text-md text-text" />
      )}
      {milestone.projects.length > 0 && (
        <ul className="m-0 flex list-none flex-wrap gap-x-4 gap-y-1 p-0 text-sm text-secondary">
          {milestone.projects.map((p) => (
            <li key={p.id}>
              <span className="font-mono">{p.key}</span> {progressWords(p.done, p.cards)}
            </li>
          ))}
        </ul>
      )}
      {detail.isError ? (
        <p className="m-0 text-sm text-secondary">The cards could not be read.</p>
      ) : (
        <ul
          className="m-0 flex list-none flex-col gap-1 p-0"
          aria-label={`Cards of ${milestone.name}`}
        >
          {(detail.data?.cards ?? []).map((card) => (
            <li key={card.id} className="flex min-w-0 items-baseline gap-2 text-sm">
              <Link
                className="min-w-0 flex-1 truncate text-text underline decoration-hairline-strong underline-offset-4 hover:decoration-current"
                to={cardPath(card.repository_id, card.id)}
              >
                {card.title}
              </Link>
              <span className="shrink-0 text-secondary">{card.done ? "Done" : card.column}</span>
            </li>
          ))}
          {detail.data?.cards.length === 0 && (
            <li className="text-sm text-secondary">No card yet. Put one in from its properties.</li>
          )}
        </ul>
      )}
    </div>
  );
}
