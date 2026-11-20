/**
 * The Milestones page (pattern 15 of the design drafts): the goals of the organisation, each with its target date and
 * how far its cards are, as a list by default and as a timeline in Display. A milestone holds cards of any projects; the
 * server counts and lists only the cards of the projects the person may see, so every number here is theirs. A row opens
 * to its description, its share in each project and its cards; a card is put in a milestone from its properties.
 */
import { FlagIcon, PencilSimpleIcon, PlusIcon, TrashIcon } from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { Link } from "react-router";
import {
  addMilestone,
  deleteMilestone,
  editMilestone,
  listMilestones,
  readMilestone,
} from "../../api/calls/milestones";
import { listProjects } from "../../api/calls/projects";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { MilestoneFields, MilestoneView } from "../../api/types/milestones";
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
import { FilterBar } from "../../ui/filters/FilterBar";
import { displayQuery, readDisplay } from "../../ui/filters/model";
import { useQueryParams } from "../../ui/filters/useQueryParams";
import { cn } from "../../ui/lib/cn";
import { ConfirmDelete } from "../../ui/page/ConfirmDelete";
import { Empty } from "../../ui/page/Empty";
import { Field, TextArea } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonList } from "../../ui/page/Skeleton";
import { cardPath } from "../pages";
import {
  dateWords,
  inProject,
  overdue,
  percent,
  progressWords,
  projectsWords,
  SORTS,
  sorted,
  timeline,
} from "./model";

/** What Display offers here: the two layouts and the sorts; milestones have no grouping and no fields to hide. */
const CHOICES = {
  layouts: [
    ["list", "List"],
    ["timeline", "Timeline"],
  ] as Array<[string, string]>,
  groups: [] as Array<[string, string]>,
  sorts: SORTS,
  fields: [] as Array<[string, string]>,
};

/** Today as `YYYY-MM-DD`, in the person's own time zone. */
function today(): string {
  const now = new Date();
  const two = (n: number) => String(n).padStart(2, "0");
  return `${now.getFullYear()}-${two(now.getMonth() + 1)}-${two(now.getDate())}`;
}

export function MilestonesPage() {
  const data = useQuery({ queryKey: keys.milestones, queryFn: listMilestones });
  const projects = useQuery({ queryKey: keys.projects, queryFn: listProjects });
  const [params, setQuery] = useQueryParams();
  const display = useMemo(() => readDisplay(params, CHOICES), [params]);
  const layout = display.layout === "timeline" ? "timeline" : "list";
  const project = params.get("project") ?? "";
  const [editing, setEditing] = useState<MilestoneView | "new" | null>(null);
  const [deleting, setDeleting] = useState<MilestoneView | null>(null);
  const queryClient = useQueryClient();
  const now = today();

  const header = (
    <PageHeader
      title="Milestones"
      context="Goals of the organisation with a target date. A milestone holds cards of any projects, and counts the ones you may see."
      actions={
        <Button size="sm" onClick={() => setEditing("new")}>
          <PlusIcon className="size-4" weight="bold" aria-hidden="true" />
          New milestone
        </Button>
      }
    />
  );
  if (data.isPending)
    return (
      <>
        <PageHeader title="Milestones" />
        <SkeletonList rows={3} message="Reading the milestones" />
      </>
    );
  if (data.isError) {
    return (
      <>
        <PageHeader title="Milestones" />
        <ReadFailed
          error={data.error}
          fallback="The milestones could not be read. Try again."
          busy={data.isFetching}
          onRetry={() => void data.refetch()}
        />
      </>
    );
  }
  const shown = sorted(inProject(data.data, project), display.sort);

  return (
    <>
      {header}
      {data.data.length === 0 ? (
        <Empty
          title="No milestone yet"
          body="A milestone is a goal with a target date. Make one, then put cards of any project in it from the card's properties."
          action={
            <Button size="sm" onClick={() => setEditing("new")}>
              New milestone
            </Button>
          }
        />
      ) : (
        <>
          <FilterBar
            label="Filter the milestones"
            fields={[
              {
                key: "project",
                label: "Project",
                options: (projects.data ?? []).map((p) => [String(p.id), p.name]),
              },
            ]}
            values={{ project }}
            onFilter={(key, value) => setQuery({ [key]: value })}
            onClear={() =>
              setQuery({ project: "", ...displayQuery({ ...display, layout: "board", sort: "" }) })
            }
            display={{
              choices: CHOICES,
              value: { ...display, layout },
              onChange: (next) => setQuery(displayQuery(next)),
            }}
          />
          {shown.length === 0 ? (
            <Empty title="No milestone matches" body="No milestone holds a card of that project." />
          ) : layout === "timeline" ? (
            <Timeline list={shown} now={now} onOpen={setEditing} />
          ) : (
            <ul className="m-0 flex list-none flex-col gap-2 p-0" aria-label="Milestones">
              {shown.map((milestone) => (
                <Row
                  key={milestone.id}
                  milestone={milestone}
                  now={now}
                  onEdit={() => setEditing(milestone)}
                  onDelete={() => setDeleting(milestone)}
                />
              ))}
            </ul>
          )}
        </>
      )}
      <MilestoneDialog editing={editing} onClose={() => setEditing(null)} />
      {deleting && (
        <ConfirmDelete
          open
          onOpenChange={(open) => !open && setDeleting(null)}
          title={`Delete ${deleting.name}?`}
          onConfirm={async () => {
            await deleteMilestone(deleting.id);
            void queryClient.invalidateQueries({ queryKey: keys.milestones });
            void queryClient.invalidateQueries({ queryKey: keys.cards });
          }}
        >
          The milestone goes. Its {deleting.cards}{" "}
          {deleting.cards === 1 ? "card stays" : "cards stay"}, in their projects.
        </ConfirmDelete>
      )}
    </>
  );
}

/** The bar of progress: the part done, with the words the bar stands for as its name. */
function Progress({ done, cards, className }: { done: number; cards: number; className?: string }) {
  const value = percent(done, cards);
  return (
    <div
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={value}
      aria-label={progressWords(done, cards)}
      className={cn("h-1.5 overflow-hidden rounded-full bg-sunken", className)}
    >
      <div className="h-full bg-complete" style={{ width: `${value}%` }} />
    </div>
  );
}

/** One milestone of the list: its name, projects and date, its progress, and under it, once opened, its cards. */
function Row({
  milestone,
  now,
  onEdit,
  onDelete,
}: {
  milestone: MilestoneView;
  now: string;
  onEdit: () => void;
  onDelete: () => void;
}) {
  const [open, setOpen] = useState(false);
  const late = overdue(milestone, now);
  return (
    <li className="rounded-card bg-raised shadow-card" data-testid="milestone">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className="grid w-full cursor-pointer appearance-none grid-cols-[1fr_auto] items-center gap-x-4 gap-y-2 rounded-card bg-transparent p-3 text-left sm:grid-cols-[minmax(0,1fr)_14rem_auto]"
      >
        <span className="flex min-w-0 items-center gap-2">
          <FlagIcon className="size-4 shrink-0 text-secondary" aria-hidden="true" />
          <span className="min-w-0 truncate text-base font-semibold text-text">
            {milestone.name}
          </span>
          <span className="shrink-0 text-sm text-secondary">{projectsWords(milestone)}</span>
        </span>
        <span className={cn("shrink-0 text-sm", late ? "text-attention-text" : "text-secondary")}>
          {late
            ? `Overdue · ${dateWords(milestone.target_date, now)}`
            : dateWords(milestone.target_date, now)}
        </span>
        <span className="col-span-2 flex items-center gap-3 sm:col-span-1 sm:w-56">
          <Progress done={milestone.done} cards={milestone.cards} className="flex-1" />
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
function Detail({
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
    <div className="flex flex-col gap-3 border-0 border-t border-solid border-hairline p-3">
      {milestone.description !== "" && (
        <p className="m-0 max-w-prose whitespace-pre-wrap text-md text-text">
          {milestone.description}
        </p>
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
      <div className="flex gap-2">
        <Button size="sm" variant="secondary" onClick={onEdit}>
          <PencilSimpleIcon className="size-4" aria-hidden="true" />
          Edit
        </Button>
        <Button size="sm" variant="ghost" onClick={onDelete}>
          <TrashIcon className="size-4" aria-hidden="true" />
          Delete
        </Button>
      </div>
    </div>
  );
}

/** The timeline: a row for each milestone, its bar from the day it was made to its target date, and a line at today. */
function Timeline({
  list,
  now,
  onOpen,
}: {
  list: MilestoneView[];
  now: string;
  onOpen: (milestone: MilestoneView) => void;
}) {
  const t = timeline(list, now);
  return (
    <div className="overflow-hidden rounded-card bg-raised shadow-card">
      <table aria-label="Milestones on a timeline" className="w-full table-fixed border-collapse">
        <thead>
          <tr className="border-0 border-b border-solid border-hairline">
            <th scope="col" className="w-44 px-3 py-2 text-left text-sm font-medium text-secondary">
              Milestone
            </th>
            <th
              scope="col"
              className="relative h-8 p-0 text-left text-sm font-normal text-secondary"
            >
              {t.months.map((month) => (
                <span
                  key={month.label}
                  className="absolute top-2 pl-1"
                  style={{ left: `${month.left}%` }}
                >
                  {month.label}
                </span>
              ))}
            </th>
          </tr>
        </thead>
        <tbody>
          {t.bars.map((bar) => (
            <tr
              key={bar.milestone.id}
              className="border-0 border-b border-solid border-hairline last:border-b-0"
            >
              <th scope="row" className="min-w-0 px-3 py-2 text-left font-normal">
                <div className="truncate text-base text-text">{bar.milestone.name}</div>
                <div className="truncate text-xs text-secondary">
                  {projectsWords(bar.milestone)} · {dateWords(bar.milestone.target_date, now)}
                </div>
              </th>
              <td className="relative h-12 p-0">
                <span
                  aria-hidden
                  className="absolute inset-y-0 w-px bg-progress"
                  style={{ left: `${t.today}%` }}
                />
                <button
                  type="button"
                  title={`${bar.milestone.name}, ${progressWords(bar.milestone.done, bar.milestone.cards)}`}
                  aria-label={`${bar.milestone.name}: ${progressWords(bar.milestone.done, bar.milestone.cards)}, due ${bar.milestone.target_date}`}
                  onClick={() => onOpen(bar.milestone)}
                  className="absolute top-3.5 h-5 cursor-pointer appearance-none overflow-hidden rounded-full border-0 bg-selected p-0"
                  style={{ left: `${bar.left}%`, width: `${bar.width}%` }}
                >
                  <span
                    className="block h-full bg-complete/60"
                    style={{ width: `${bar.progress}%` }}
                  />
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** The dialog that makes a milestone or edits one: its name, its target date and its description. */
function MilestoneDialog({
  editing,
  onClose,
}: {
  editing: MilestoneView | "new" | null;
  onClose: () => void;
}) {
  const queryClient = useQueryClient();
  const initial: MilestoneFields =
    editing === null || editing === "new"
      ? { name: "", target_date: "", description: "" }
      : { name: editing.name, target_date: editing.target_date, description: editing.description };
  return (
    <Dialog open={editing !== null} onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        {editing !== null && (
          <MilestoneForm
            key={editing === "new" ? "new" : editing.id}
            initial={initial}
            id={editing === "new" ? null : editing.id}
            onDone={() => {
              void queryClient.invalidateQueries({ queryKey: keys.milestones });
              void queryClient.invalidateQueries({ queryKey: keys.cards });
              onClose();
            }}
            onCancel={onClose}
          />
        )}
      </DialogContent>
    </Dialog>
  );
}

function MilestoneForm({
  initial,
  id,
  onDone,
  onCancel,
}: {
  initial: MilestoneFields;
  id: number | null;
  onDone: () => void;
  onCancel: () => void;
}) {
  const [fields, setFields] = useState(initial);
  const save = useMutation({
    mutationFn: () =>
      id === null
        ? addMilestone({ ...fields, name: fields.name.trim() })
        : editMilestone(id, { ...fields, name: fields.name.trim() }),
    onSuccess: onDone,
  });
  const ready = fields.name.trim() !== "" && fields.target_date !== "" && !save.isPending;
  return (
    <form
      className="flex flex-col gap-4"
      onSubmit={(event) => {
        event.preventDefault();
        if (ready) save.mutate();
      }}
    >
      <DialogHeader>
        <DialogTitle>{id === null ? "New milestone" : "Edit milestone"}</DialogTitle>
        <DialogDescription>
          A goal of the organisation. Put cards of any project in it from the card's properties.
        </DialogDescription>
      </DialogHeader>
      <Field label="Name">
        <Input
          value={fields.name}
          placeholder="Launch 2.0"
          autoComplete="off"
          onChange={(event) => setFields({ ...fields, name: event.target.value })}
        />
      </Field>
      <Field label="Target date">
        <Input
          type="date"
          value={fields.target_date}
          onChange={(event) => setFields({ ...fields, target_date: event.target.value })}
        />
      </Field>
      <Field label="Description">
        <TextArea
          value={fields.description}
          rows={3}
          onChange={(event) => setFields({ ...fields, description: event.target.value })}
        />
      </Field>
      {save.isError && (
        <Notice tone="error">
          <span className="font-semibold">Couldn't save the milestone. </span>
          {sayError(save.error, "Try again.")}
        </Notice>
      )}
      <DialogFooter>
        <Button type="button" variant="secondary" onClick={onCancel}>
          Cancel
        </Button>
        <Button type="submit" disabled={!ready}>
          {save.isPending ? "Saving" : id === null ? "Make milestone" : "Save"}
        </Button>
      </DialogFooter>
    </form>
  );
}
