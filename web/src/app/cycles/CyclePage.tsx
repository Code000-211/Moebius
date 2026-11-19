/**
 * A cycle's own page on the Cycles page: its figures and burn-up above the list of its cards, and what a member does
 * with it (edit, close, delete).
 */
import { ArrowLeftIcon, PencilSimpleIcon, TrashIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { deleteCycle, readCycle } from "../../api/calls/cycles";
import { keys } from "../../api/keys";
import type { CycleDetail, CycleView } from "../../api/types/cycles";
import { Button } from "../../ui/components/Button";
import { ConfirmDelete } from "../../ui/page/ConfirmDelete";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonList } from "../../ui/page/Skeleton";
import { CloseDialog } from "./CycleDialogs";
import { Progress, STATE_WORDS } from "./CycleProgress";
import { chart, periodWords, perPointWords, progressWords, stateOf } from "./model";

/** A cycle's page: its figures and burn-up above the list of its cards, and what a member does with it. */
export function CyclePage({
  cycle,
  all,
  now,
  mayChange,
  chooser,
  onBack,
  onEdit,
  onChanged,
}: {
  cycle: CycleView;
  all: CycleView[];
  now: string;
  mayChange: boolean;
  chooser: React.ReactNode;
  onBack: () => void;
  onEdit: () => void;
  onChanged: () => void;
}) {
  const detail = useQuery({ queryKey: keys.cycle(cycle.id), queryFn: () => readCycle(cycle.id) });
  const [closing, setClosing] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const state = stateOf(cycle, now);
  const header = (
    <PageHeader
      title={cycle.name}
      context={`${periodWords(cycle, now)} · ${STATE_WORDS[state]}`}
      back={
        <Button size="icon" variant="ghost" aria-label="All cycles" onClick={onBack}>
          <ArrowLeftIcon className="size-4" aria-hidden="true" />
        </Button>
      }
      actions={
        <>
          {chooser}
          {mayChange && cycle.closed_at === null && (
            <>
              <Button size="sm" variant="secondary" onClick={onEdit}>
                <PencilSimpleIcon className="size-4" aria-hidden="true" />
                Edit
              </Button>
              <Button size="sm" onClick={() => setClosing(true)}>
                Close cycle
              </Button>
            </>
          )}
          {mayChange && (
            <Button size="sm" variant="ghost" onClick={() => setDeleting(true)}>
              <TrashIcon className="size-4" aria-hidden="true" />
              Delete
            </Button>
          )}
        </>
      }
    />
  );
  return (
    <>
      {header}
      <Progress cycle={cycle} className="mb-2" />
      <p className="mt-0 mb-4 text-sm text-secondary">{progressWords(cycle)}</p>
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
      <dl className="m-0 mb-4 grid grid-cols-2 gap-2 sm:grid-cols-4">
        {tiles.map(([name, value]) => (
          <div key={name} className="rounded-card bg-raised p-3 shadow-card">
            <dt className="text-sm text-secondary">{name}</dt>
            <dd className="m-0 text-lg font-semibold text-text tabular-nums">{value}</dd>
          </div>
        ))}
      </dl>
      <BurnUp detail={detail} />
      <ul
        className="m-0 mt-4 flex list-none flex-col gap-1 p-0"
        aria-label={`Cards of ${detail.name}`}
      >
        {detail.cards.map((card) => (
          <li
            key={card.id}
            className="flex min-w-0 items-baseline gap-2 rounded-card bg-raised px-3 py-2 text-sm shadow-card"
          >
            <span className="min-w-0 flex-1 truncate text-text">{card.title}</span>
            <span className="shrink-0 text-secondary tabular-nums">
              {card.estimate === null ? "No estimate" : `${card.estimate} pt`}
            </span>
            <span className="shrink-0 text-secondary">{card.done ? "Done" : card.column}</span>
          </li>
        ))}
        {detail.cards.length === 0 && (
          <li className="text-sm text-secondary">No card yet. Put one in from its properties.</li>
        )}
      </ul>
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
    <figure className="m-0 rounded-card bg-raised p-3 shadow-card">
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
