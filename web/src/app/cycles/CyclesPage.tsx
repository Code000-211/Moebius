/**
 * The Cycles page (pattern 8 of the design drafts): the time boxes of one project, chosen at the top as on Intake. The
 * list shows each cycle with how far its cards are; a cycle opens to its own page, with the burn-up and the figures
 * (done points, cards done by agents, tokens per point) above the list of its cards. A member makes, edits, closes and
 * deletes cycles; closing asks where the cards that are not done go, to the next cycle or out, and a closed cycle keeps
 * its cards. A project need not use cycles: it simply has none. A card is put in a cycle from its properties.
 */
import { PlusIcon } from "@phosphor-icons/react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { listCycles } from "../../api/calls/cycles";
import { listProjects } from "../../api/calls/projects";
import { keys } from "../../api/keys";
import type { CycleView } from "../../api/types/cycles";
import { Button } from "../../ui/components/Button";
import { useQueryParams } from "../../ui/filters/useQueryParams";
import { cn } from "../../ui/lib/cn";
import { Empty } from "../../ui/page/Empty";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonList } from "../../ui/page/Skeleton";
import { decides } from "../intake/model";
import { useRole } from "../shell/session";
import { CycleDialog } from "./CycleDialogs";
import { CyclePage } from "./CyclePage";
import { Progress, STATE_WORDS } from "./CycleProgress";
import { ordered, periodWords, progressWords, stateOf } from "./model";

/** Today as `YYYY-MM-DD`, in the person's own time zone. */
function today(): string {
  const now = new Date();
  const two = (n: number) => String(n).padStart(2, "0");
  return `${now.getFullYear()}-${two(now.getMonth() + 1)}-${two(now.getDate())}`;
}

export function CyclesPage() {
  const projects = useQuery({ queryKey: keys.projects, queryFn: listProjects });
  const [params, setQuery] = useQueryParams();
  if (projects.isPending) {
    return (
      <>
        <PageHeader title="Cycles" />
        <SkeletonList rows={3} message="Reading the projects" />
      </>
    );
  }
  if (projects.isError) {
    return (
      <>
        <PageHeader title="Cycles" />
        <ReadFailed
          error={projects.error}
          fallback="The projects could not be read. Try again."
          busy={projects.isFetching}
          onRetry={() => void projects.refetch()}
        />
      </>
    );
  }
  const chosen = Number(params.get("project"));
  const project = projects.data.find((each) => each.id === chosen) ?? projects.data[0];
  if (project === undefined) {
    return (
      <>
        <PageHeader title="Cycles" />
        <Empty title="No project yet" body="Add a project, and it can plan its work in cycles." />
      </>
    );
  }
  const opened = Number(params.get("cycle")) || null;
  return (
    <ProjectCycles
      key={project.id}
      project={project.id}
      opened={opened}
      onOpen={(cycle) => setQuery({ cycle: cycle === null ? "" : String(cycle) })}
      chooser={
        <select
          aria-label="Project"
          className="h-8 rounded-control bg-field px-2 text-base text-text"
          value={project.id}
          onChange={(event) => setQuery({ project: event.target.value, cycle: "" })}
        >
          {projects.data.map((each) => (
            <option key={each.id} value={each.id}>
              {each.name}
            </option>
          ))}
        </select>
      }
    />
  );
}

function ProjectCycles({
  project,
  opened,
  onOpen,
  chooser,
}: {
  project: number;
  opened: number | null;
  onOpen: (cycle: number | null) => void;
  chooser: React.ReactNode;
}) {
  const role = useRole();
  const queryClient = useQueryClient();
  const cycles = useQuery({
    queryKey: keys.projectCycles(project),
    queryFn: () => listCycles(project),
  });
  const [editing, setEditing] = useState<CycleView | "new" | null>(null);
  const now = today();
  const mayChange = decides(role);
  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: keys.cycles });
    void queryClient.invalidateQueries({ queryKey: keys.cards });
  };

  const context = "A cycle is a time box of the project, with the cards it means to finish in it.";
  if (cycles.isPending) {
    return (
      <>
        <PageHeader title="Cycles" actions={chooser} />
        <SkeletonList rows={3} message="Reading the cycles" />
      </>
    );
  }
  if (cycles.isError) {
    return (
      <>
        <PageHeader title="Cycles" actions={chooser} />
        <ReadFailed
          error={cycles.error}
          fallback="The cycles could not be read. Try again."
          busy={cycles.isFetching}
          onRetry={() => void cycles.refetch()}
        />
      </>
    );
  }
  const open = opened === null ? undefined : cycles.data.find((c) => c.id === opened);
  const newButton = mayChange && (
    <Button size="sm" onClick={() => setEditing("new")}>
      <PlusIcon className="size-4" weight="bold" aria-hidden="true" />
      New cycle
    </Button>
  );
  return (
    <>
      {open ? (
        <CyclePage
          cycle={open}
          all={cycles.data}
          now={now}
          mayChange={mayChange}
          chooser={chooser}
          onBack={() => onOpen(null)}
          onEdit={() => setEditing(open)}
          onChanged={refresh}
        />
      ) : (
        <>
          <PageHeader
            title="Cycles"
            context={context}
            actions={
              <>
                {chooser}
                {newButton}
              </>
            }
          />
          {cycles.data.length === 0 ? (
            <Empty
              title="No cycle yet"
              body="A project need not use cycles. Make one to plan a stretch of work, then put cards in it from the card's properties."
              action={mayChange ? newButton : undefined}
            />
          ) : (
            <ul className="m-0 flex list-none flex-col gap-2 p-0" aria-label="Cycles">
              {ordered(cycles.data).map((cycle) => (
                <Row key={cycle.id} cycle={cycle} now={now} onOpen={() => onOpen(cycle.id)} />
              ))}
            </ul>
          )}
        </>
      )}
      <CycleDialog
        editing={editing}
        project={project}
        onClose={() => setEditing(null)}
        onDone={refresh}
      />
    </>
  );
}

/** One cycle of the list: its name and dates, where it stands and its progress; a click opens its page. */
function Row({ cycle, now, onOpen }: { cycle: CycleView; now: string; onOpen: () => void }) {
  const state = stateOf(cycle, now);
  return (
    <li className="rounded-card bg-raised shadow-card" data-testid="cycle">
      <button
        type="button"
        onClick={onOpen}
        className="grid w-full cursor-pointer appearance-none grid-cols-[1fr_auto] items-center gap-x-4 gap-y-2 rounded-card bg-transparent p-3 text-left sm:grid-cols-[minmax(0,1fr)_12rem_16rem]"
      >
        <span className="min-w-0 truncate text-base font-semibold text-text">{cycle.name}</span>
        <span
          className={cn(
            "shrink-0 text-sm",
            state === "overdue" ? "text-attention-text" : "text-secondary",
          )}
        >
          {periodWords(cycle, now)} · {STATE_WORDS[state]}
        </span>
        <span className="col-span-2 flex items-center gap-3 sm:col-span-1">
          <Progress cycle={cycle} className="flex-1" />
          <span className="shrink-0 text-sm text-secondary tabular-nums">
            {progressWords(cycle)}
          </span>
        </span>
      </button>
    </li>
  );
}
