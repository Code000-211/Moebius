/**
 * The Cycles page (pattern 8 of the design drafts): the time boxes of one project, chosen at the top as on Intake. The
 * list shows each cycle with how far its cards are; a cycle opens to its own page, with the burn-up and the figures
 * (done points, cards done by agents, tokens per point) above the list of its cards. A member makes, edits, closes and
 * deletes cycles; closing asks where the cards that are not done go, to the next cycle or out, and a closed cycle keeps
 * its cards. A project need not use cycles: it simply has none. A card is put in a cycle from its properties.
 */
import { ArrowLeftIcon, PencilSimpleIcon, PlusIcon, TrashIcon } from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useId, useState } from "react";
import {
  addCycle,
  closeCycle,
  deleteCycle,
  editCycle,
  listCycles,
  readCycle,
} from "../../api/calls/cycles";
import { listProjects } from "../../api/calls/projects";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { Carry, CycleDetail, CycleFields, CycleView } from "../../api/types/cycles";
import { Button } from "../../ui/components/Button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../../ui/components/Dialog";
import { Input } from "../../ui/components/Input";
import { useQueryParams } from "../../ui/filters/useQueryParams";
import { cn } from "../../ui/lib/cn";
import { ConfirmDelete } from "../../ui/page/ConfirmDelete";
import { Empty } from "../../ui/page/Empty";
import { Field } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonList } from "../../ui/page/Skeleton";
import { decides } from "../intake/model";
import { useRole } from "../shell/session";
import {
  chart,
  nextCycle,
  ordered,
  percent,
  periodWords,
  perPointWords,
  progressWords,
  stateOf,
} from "./model";

/** Today as `YYYY-MM-DD`, in the person's own time zone. */
function today(): string {
  const now = new Date();
  const two = (n: number) => String(n).padStart(2, "0");
  return `${now.getFullYear()}-${two(now.getMonth() + 1)}-${two(now.getDate())}`;
}

const STATE_WORDS = {
  closed: "Closed",
  upcoming: "Upcoming",
  running: "Running",
  overdue: "Past its end",
} as const;

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

/** The bar of progress: the part done, with the words the bar stands for as its name. */
function Progress({ cycle, className }: { cycle: CycleView; className?: string }) {
  const value = percent(cycle);
  return (
    <div
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={value}
      aria-label={progressWords(cycle)}
      className={cn("h-1.5 overflow-hidden rounded-full bg-sunken", className)}
    >
      <div className="h-full bg-complete" style={{ width: `${value}%` }} />
    </div>
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

/** A cycle's page: its figures and burn-up above the list of its cards, and what a member does with it. */
function CyclePage({
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

/** The dialog that makes a cycle of the project or edits one: its name, its first and its last day. */
function CycleDialog({
  editing,
  project,
  onClose,
  onDone,
}: {
  editing: CycleView | "new" | null;
  project: number;
  onClose: () => void;
  onDone: () => void;
}) {
  return (
    <Dialog open={editing !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        {editing !== null && (
          <CycleForm
            key={editing === "new" ? "new" : editing.id}
            project={project}
            editing={editing === "new" ? null : editing}
            onDone={() => {
              onDone();
              onClose();
            }}
            onCancel={onClose}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

function CycleForm({
  project,
  editing,
  onDone,
  onCancel,
}: {
  project: number;
  editing: CycleView | null;
  onDone: () => void;
  onCancel: () => void;
}) {
  const [fields, setFields] = useState<CycleFields>(
    editing
      ? { name: editing.name, starts_on: editing.starts_on, ends_on: editing.ends_on }
      : { name: "", starts_on: "", ends_on: "" },
  );
  const save = useMutation({
    mutationFn: () => {
      const body = { ...fields, name: fields.name.trim() };
      return editing === null ? addCycle(project, body) : editCycle(editing.id, body);
    },
    onSuccess: onDone,
  });
  const ready =
    fields.name.trim() !== "" &&
    fields.starts_on !== "" &&
    fields.ends_on !== "" &&
    !save.isPending;
  return (
    <form
      className="flex flex-col gap-4"
      onSubmit={(event) => {
        event.preventDefault();
        if (ready) save.mutate();
      }}
    >
      <DialogHeader>
        <DialogTitle>{editing === null ? "New cycle" : "Edit cycle"}</DialogTitle>
        <DialogDescription>
          A time box of this project. Put its cards in from the card's properties.
        </DialogDescription>
      </DialogHeader>
      <Field label="Name">
        <Input
          value={fields.name}
          placeholder="Cycle 12"
          autoComplete="off"
          onChange={(event) => setFields({ ...fields, name: event.target.value })}
        />
      </Field>
      <Field label="First day">
        <Input
          type="date"
          value={fields.starts_on}
          onChange={(event) => setFields({ ...fields, starts_on: event.target.value })}
        />
      </Field>
      <Field label="Last day">
        <Input
          type="date"
          value={fields.ends_on}
          onChange={(event) => setFields({ ...fields, ends_on: event.target.value })}
        />
      </Field>
      {save.isError && (
        <Notice tone="error">
          <span className="font-semibold">Couldn't save the cycle. </span>
          {sayError(save.error, "Try again.")}
        </Notice>
      )}
      <DialogFooter>
        <Button type="button" variant="secondary" onClick={onCancel}>
          Cancel
        </Button>
        <Button type="submit" disabled={!ready}>
          {save.isPending ? "Saving" : editing === null ? "Make cycle" : "Save"}
        </Button>
      </DialogFooter>
    </form>
  );
}

/** The dialog that closes a cycle: the person chooses where the cards that are not done go. */
function CloseDialog({
  cycle,
  all,
  onClose,
  onDone,
}: {
  cycle: CycleView;
  all: CycleView[];
  onClose: () => void;
  onDone: () => void;
}) {
  const name = useId();
  const others = all
    .filter((c) => c.id !== cycle.id && c.closed_at === null)
    .sort((a, b) => a.starts_on.localeCompare(b.starts_on) || a.id - b.id);
  const next = nextCycle(all, cycle);
  const [carry, setCarry] = useState<"next" | "out">(next ? "next" : "out");
  const [to, setTo] = useState<number | undefined>(next?.id);
  const unfinished = cycle.cards - cycle.done;
  const close = useMutation({
    mutationFn: () => {
      const body: Carry =
        carry === "out" || to === undefined ? { carry: "out" } : { carry: "next", to };
      return closeCycle(cycle.id, body);
    },
    onSuccess: onDone,
  });
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        <form
          className="flex flex-col gap-4"
          onSubmit={(event) => {
            event.preventDefault();
            close.mutate();
          }}
        >
          <DialogHeader>
            <DialogTitle>Close {cycle.name}</DialogTitle>
            <DialogDescription>
              {unfinished === 0
                ? "Every card is done. The cycle keeps its cards as its record."
                : `${unfinished} ${unfinished === 1 ? "card is" : "cards are"} not done. Choose where ${unfinished === 1 ? "it goes" : "they go"}; the cycle keeps its cards as its record.`}
            </DialogDescription>
          </DialogHeader>
          <fieldset className="m-0 flex flex-col gap-2 border-0 p-0">
            <legend className="sr-only">Cards that are not done</legend>
            <label className="flex items-center gap-2 text-base text-text">
              <input
                type="radio"
                name={name}
                checked={carry === "next"}
                disabled={others.length === 0}
                onChange={() => setCarry("next")}
              />
              Move them to
              <select
                aria-label="Next cycle"
                className="h-8 rounded-control bg-field px-2 text-base text-text"
                value={to ?? ""}
                disabled={others.length === 0}
                onChange={(event) => {
                  setTo(Number(event.target.value));
                  setCarry("next");
                }}
              >
                {others.length === 0 && <option value="">No open cycle</option>}
                {others.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </select>
            </label>
            <label className="flex items-center gap-2 text-base text-text">
              <input
                type="radio"
                name={name}
                checked={carry === "out"}
                onChange={() => setCarry("out")}
              />
              Take them out of any cycle
            </label>
          </fieldset>
          {close.isError && (
            <Notice tone="error">
              <span className="font-semibold">Couldn't close the cycle. </span>
              {sayError(close.error, "Try again.")}
            </Notice>
          )}
          <DialogFooter>
            <Button type="button" variant="secondary" onClick={onClose}>
              Cancel
            </Button>
            <Button type="submit" disabled={close.isPending}>
              {close.isPending ? "Closing" : "Close cycle"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
