/**
 * The Cycles tab of a project (pattern 8 of the design drafts): its time boxes. The list shows each cycle with how far
 * its cards are; a cycle opens to its own page (`/projects/:id/cycles/:cycleId`), with the burn-up and the figures (done
 * points, cards done by agents, tokens per point) above the list of its cards. A member makes, edits, closes and
 * deletes cycles; closing asks where the cards that are not done go, to the next cycle or out, and a closed cycle keeps
 * its cards. A project need not use cycles: it simply has none. A card is put in a cycle from its properties.
 */
import { PlusIcon } from "@phosphor-icons/react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link, useNavigate } from "react-router";
import { listCycles } from "../../api/calls/cycles";
import { keys } from "../../api/keys";
import type { CycleView } from "../../api/types/cycles";
import type { Project } from "../../api/types/projects";
import { Button } from "../../ui/components/Button";
import { cn } from "../../ui/lib/cn";
import { Empty } from "../../ui/page/Empty";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonList } from "../../ui/page/Skeleton";
import { axisOf, TimeBar } from "../../ui/page/TimeBar";
import { decides } from "../intake/model";
import { cyclePath, projectTabPath } from "../pages";
import { ProjectHeader } from "../projects/ProjectHeader";
import { useRole } from "../shell/session";
import { CycleDialog } from "./CycleDialogs";
import { CyclePage } from "./CyclePage";
import { STATE_WORDS } from "./CycleProgress";
import { ordered, percent, periodWords, progressWords, stateOf } from "./model";

/** Today as `YYYY-MM-DD`, in the person's own time zone. */
function today(): string {
  const now = new Date();
  const two = (n: number) => String(n).padStart(2, "0");
  return `${now.getFullYear()}-${two(now.getMonth() + 1)}-${two(now.getDate())}`;
}

export function CyclesTab({
  project,
  cycleId,
  onTerminal,
}: {
  project: Project;
  /** The cycle whose page is open (`/projects/:id/cycles/:cycleId`), if any. */
  cycleId: number | null;
  onTerminal: () => void;
}) {
  const navigate = useNavigate();
  const role = useRole();
  const queryClient = useQueryClient();
  const cycles = useQuery({
    queryKey: keys.projectCycles(project.id),
    queryFn: () => listCycles(project.id),
  });
  const [editing, setEditing] = useState<CycleView | "new" | null>(null);
  const now = today();
  const mayChange = decides(role);
  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: keys.cycles });
    void queryClient.invalidateQueries({ queryKey: keys.cards });
  };
  const header = (actions?: React.ReactNode) => (
    <ProjectHeader project={project} tab="cycles" actions={actions} onTerminal={onTerminal} />
  );

  if (cycles.isPending) {
    return (
      <>
        {header()}
        <SkeletonList rows={3} message="Reading the cycles" />
      </>
    );
  }
  if (cycles.isError) {
    return (
      <>
        {header()}
        <ReadFailed
          error={cycles.error}
          fallback="The cycles could not be read. Try again."
          busy={cycles.isFetching}
          onRetry={() => void cycles.refetch()}
        />
      </>
    );
  }
  const open = cycleId === null ? undefined : cycles.data.find((c) => c.id === cycleId);
  const back = () => navigate(projectTabPath(project.id, "cycles"));
  const newButton = mayChange && (
    <Button size="sm" onClick={() => setEditing("new")}>
      <PlusIcon className="size-4" weight="bold" aria-hidden="true" />
      New cycle
    </Button>
  );
  return (
    <>
      {cycleId !== null && !open ? (
        <>
          {header()}
          <Empty
            title="No such cycle"
            body="This project has no cycle with that address. It may have been deleted, or belong to another project."
            action={
              <Button asChild size="sm">
                <Link to={projectTabPath(project.id, "cycles")}>All cycles</Link>
              </Button>
            }
          />
        </>
      ) : open ? (
        <CyclePage
          project={project}
          cycle={open}
          all={cycles.data}
          now={now}
          mayChange={mayChange}
          onBack={back}
          onEdit={() => setEditing(open)}
          onChanged={refresh}
          onTerminal={onTerminal}
        />
      ) : (
        <>
          {header(newButton)}
          {cycles.data.length === 0 ? (
            <Empty
              title="No cycle yet"
              body="A project need not use cycles. Make one to plan a stretch of work, then put cards in it from the card's properties."
              action={mayChange ? newButton : undefined}
            />
          ) : (
            <Timeline
              cycles={cycles.data}
              now={now}
              onOpen={(id) => navigate(cyclePath(project.id, id))}
            />
          )}
        </>
      )}
      <CycleDialog
        editing={editing}
        project={project.id}
        onClose={() => setEditing(null)}
        onDone={refresh}
      />
    </>
  );
}

/** The cycles as rows on one time axis: each bar spans its dates, and the one under way is marked. */
export function Timeline({
  cycles,
  now,
  onOpen,
}: {
  cycles: CycleView[];
  now: string;
  onOpen: (cycle: number) => void;
}) {
  const axis = axisOf(cycles.map((c) => ({ start: c.starts_on, end: c.ends_on })));
  if (axis === null) return null;
  return (
    <ul className="m-0 flex list-none flex-col p-0" aria-label="Cycles">
      {ordered(cycles).map((cycle) => (
        <Row key={cycle.id} cycle={cycle} now={now} axis={axis} onOpen={() => onOpen(cycle.id)} />
      ))}
    </ul>
  );
}

/** One cycle of the timeline: its name and dates, where it stands, and its bar; a click opens its page. */
function Row({
  cycle,
  now,
  axis,
  onOpen,
}: {
  cycle: CycleView;
  now: string;
  axis: { start: string; end: string };
  onOpen: () => void;
}) {
  const state = stateOf(cycle, now);
  const current = state === "running";
  return (
    <li
      className="border-b border-hairline"
      data-testid="cycle"
      aria-current={current ? "date" : undefined}
    >
      <button
        type="button"
        onClick={onOpen}
        className={cn(
          "grid min-h-9 w-full cursor-pointer appearance-none grid-cols-[1fr_auto] items-center gap-x-4 gap-y-1 bg-transparent px-3 py-2 text-left hover:bg-row-hover",
          "sm:grid-cols-[minmax(0,12rem)_minmax(0,1fr)_13rem]",
        )}
      >
        <span className="min-w-0 truncate text-base font-medium text-text">{cycle.name}</span>
        <span
          className={cn(
            "shrink-0 whitespace-nowrap text-right text-sm sm:col-start-3 sm:row-start-1",
            state === "overdue"
              ? "text-attention-text"
              : current
                ? "text-accent-text"
                : "text-secondary",
          )}
        >
          {periodWords(cycle, now)} · {current ? "Current" : STATE_WORDS[state]}
        </span>
        <span className="col-span-2 flex items-center sm:col-span-1 sm:col-start-2 sm:row-start-1">
          <TimeBar
            className="flex-1"
            start={cycle.starts_on}
            end={cycle.ends_on}
            axis={axis}
            progress={percent(cycle)}
            label={progressWords(cycle)}
            today={now}
            emphasis={current}
          />
        </span>
      </button>
    </li>
  );
}
