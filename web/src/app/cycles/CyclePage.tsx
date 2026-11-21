/**
 * A cycle's own page under the Cycles tab of its project: its figures and burn-up above the list of its cards, and what a member does
 * with it (edit, close, delete).
 */
import { ArrowLeftIcon, PencilSimpleIcon, TrashIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { deleteCycle, readCycle } from "../../api/calls/cycles";
import { keys } from "../../api/keys";
import type { CycleDetail, CycleView } from "../../api/types/cycles";
import type { Project } from "../../api/types/projects";
import { Button } from "../../ui/components/Button";
import { ConfirmDelete } from "../../ui/page/ConfirmDelete";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonList } from "../../ui/page/Skeleton";
import { TimeBar } from "../../ui/page/TimeBar";
import { CardRows } from "../board/CardRows";
import { ProjectHeader } from "../projects/ProjectHeader";
import { CloseDialog } from "./CycleDialogs";
import { STATE_WORDS } from "./CycleProgress";
import { chart, percent, periodWords, perPointWords, progressWords, stateOf } from "./model";

/** A cycle's page: its figures and burn-up above the list of its cards, and what a member does with it. */
export function CyclePage({
  project,
  cycle,
  all,
  now,
  mayChange,
  onBack,
  onEdit,
  onChanged,
  onTerminal,
}: {
  project: Project;
  cycle: CycleView;
  all: CycleView[];
  now: string;
  mayChange: boolean;
  onBack: () => void;
  onEdit: () => void;
  onChanged: () => void;
  onTerminal: () => void;
}) {
  const detail = useQuery({ queryKey: keys.cycle(cycle.id), queryFn: () => readCycle(cycle.id) });
  const [closing, setClosing] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const state = stateOf(cycle, now);
  const close = mayChange && cycle.closed_at === null && (
    <Button size="sm" onClick={() => setClosing(true)}>
      Close cycle
    </Button>
  );
  return (
    <>
      <ProjectHeader project={project} tab="cycles" primary={close} onTerminal={onTerminal} />
      <div className="mb-3 flex items-center gap-1">
        <Button
          size="icon"
          variant="ghost"
          className="-ml-2"
          aria-label="All cycles"
          onClick={onBack}
        >
          <ArrowLeftIcon className="size-4" aria-hidden="true" />
        </Button>
        <h2 className="m-0 min-w-0 truncate text-lg font-semibold text-text">{cycle.name}</h2>
        {mayChange && (
          <span className="ml-auto flex items-center gap-2">
            {cycle.closed_at === null && (
              <Button size="sm" variant="ghost" onClick={onEdit}>
                <PencilSimpleIcon className="size-4" aria-hidden="true" />
                Edit
              </Button>
            )}
            <Button size="sm" variant="ghost" onClick={() => setDeleting(true)}>
              <TrashIcon className="size-4" aria-hidden="true" />
              Delete
            </Button>
          </span>
        )}
      </div>
      <TimeBar
        className="mt-3 mb-2"
        start={cycle.starts_on}
        end={cycle.ends_on}
        axis={{ start: cycle.starts_on, end: cycle.ends_on }}
        progress={percent(cycle)}
        label={progressWords(cycle)}
        today={now}
        emphasis={state === "running"}
      />
      <p className="mt-0 mb-4 text-sm text-secondary">
        {periodWords(cycle, now)} · {state === "running" ? "Current" : STATE_WORDS[state]}
      </p>
      {detail.isPending ? (
        <SkeletonList rows={3} message="Reading the cycle" />
      ) : detail.isError ? (
        <ReadFailed
          error={detail.error}
          fallback="The cycle could not be read. Try again."
          busy={detail.isFetching}
          onRetry={() => void detail.refetch()}
        />
      ) : (
        <Body detail={detail.data} />
      )}
      {closing && (
        <CloseDialog
          cycle={cycle}
          all={all}
          onClose={() => setClosing(false)}
          onDone={() => {
            setClosing(false);
            onChanged();
          }}
        />
      )}
      {deleting && (
        <ConfirmDelete
          open
          onOpenChange={(open) => !open && setDeleting(false)}
          title={`Delete ${cycle.name}?`}
          onConfirm={async () => {
            await deleteCycle(cycle.id);
            onBack();
            onChanged();
          }}
        >
          The cycle goes. Its {cycle.cards} {cycle.cards === 1 ? "card stays" : "cards stay"}, in
          the project.
        </ConfirmDelete>
      )}
    </>
  );
}

/** The figures, the burn-up and the cards of a cycle. */
function Body({ detail }: { detail: CycleDetail }) {
  const { figures } = detail;
  const tiles: Array<[string, string]> = [
    ["Points done", `${detail.points_done} of ${detail.points}`],
    ["Cards done", `${detail.done} of ${detail.cards.length}`],
    ["Cards by agents", String(figures.by_agents)],
    ["Tokens per point", perPointWords(figures.tokens_per_point)],
  ];
  return (
    <>
      <dl className="m-0 mb-4 grid grid-cols-2 gap-x-4 gap-y-3 border-y border-hairline py-3 sm:grid-cols-4">
        {tiles.map(([name, value]) => (
          <div key={name}>
            <dt className="text-sm text-secondary">{name}</dt>
            <dd className="m-0 text-lg font-semibold text-text tabular-nums">{value}</dd>
          </div>
        ))}
      </dl>
      <BurnUp detail={detail} />
      <div className="mt-6">
        <CardRows
          label={`Cards of ${detail.name}`}
          cards={detail.cards.map((card) => ({
            ...card,
            note: card.estimate === null ? "No estimate" : `${card.estimate} pt`,
          }))}
        />
        {detail.cards.length === 0 && (
          <p className="m-0 text-sm text-secondary">No card yet. Put one in from its properties.</p>
        )}
      </div>
    </>
  );
}

/** The burn-up: the points planned and the points done, a line each, day by day. */
function BurnUp({ detail }: { detail: CycleDetail }) {
  const width = 600;
  const height = 120;
  const lines = chart(detail.burn_up, width, height);
  const last = detail.burn_up[detail.burn_up.length - 1];
  return (
    <figure className="m-0">
      <figcaption className="mb-2 text-sm text-secondary">
        Burn-up, in points
        {last ? `: ${last.done} done of ${last.scope} planned` : ": the cycle has not started"}
      </figcaption>
      {last && (
        <svg
          role="img"
          aria-label={`Burn-up over ${lines.days} ${lines.days === 1 ? "day" : "days"}: ${last.done} of ${last.scope} points done`}
          viewBox={`-4 -4 ${width + 8} ${height + 8}`}
          className="h-32 w-full"
          preserveAspectRatio="none"
        >
          <polyline
            points={lines.scope}
            fill="none"
            strokeWidth={2}
            vectorEffect="non-scaling-stroke"
            className="stroke-secondary"
            strokeDasharray="4 3"
          />
          <polyline
            points={lines.done}
            fill="none"
            strokeWidth={2}
            vectorEffect="non-scaling-stroke"
            className="stroke-complete"
          />
        </svg>
      )}
    </figure>
  );
}
