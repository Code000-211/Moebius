/**
 * The Milestones tab of a project (pattern 15 of the design drafts): the goals of the organisation that hold cards of
 * this project, plus the ones made from here, each a row on a time axis with its target date and how far this project's
 * cards are. A milestone holds cards of any projects (ADR 0019), so the same goal can be in the tab of several; one with
 * no card in any project is listed nowhere. A row opens to its description and this project's cards; a card is put in
 * a milestone from its properties.
 */
import { PlusIcon } from "@phosphor-icons/react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { deleteMilestone, listMilestones } from "../../api/calls/milestones";
import { keys } from "../../api/keys";
import type { MilestoneView } from "../../api/types/milestones";
import type { Project } from "../../api/types/projects";
import { Button } from "../../ui/components/Button";
import { DisplayMenu } from "../../ui/filters/FilterBar";
import { displayQuery, readDisplay } from "../../ui/filters/model";
import { useQueryParams } from "../../ui/filters/useQueryParams";
import { ConfirmDelete } from "../../ui/page/ConfirmDelete";
import { Empty } from "../../ui/page/Empty";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonList } from "../../ui/page/Skeleton";
import { ProjectHeader } from "../projects/ProjectHeader";
import { MilestoneDialog } from "./MilestoneDialogs";
import { Timeline } from "./MilestoneRows";
import { inProject, SORTS, sorted } from "./model";

/** What Display offers here: the sorts; milestones have one layout, no grouping and no fields to hide. */
const CHOICES = {
  layouts: [] as Array<[string, string]>,
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

export function MilestonesTab({
  project,
  onTerminal,
}: {
  project: Project;
  onTerminal: () => void;
}) {
  const data = useQuery({ queryKey: keys.milestones, queryFn: listMilestones });
  const [params, setQuery] = useQueryParams();
  const displayValue = useMemo(() => readDisplay(params, CHOICES), [params]);
  // The milestone that is opened is in the address, so a reload and a link keep it open.
  const opened = Number(params.get("milestone")) || null;
  const [editing, setEditing] = useState<MilestoneView | "new" | null>(null);
  const [deleting, setDeleting] = useState<MilestoneView | null>(null);
  // A milestone made here has no card yet, so only this list remembers it until a card is put in it.
  const [madeHere, setMadeHere] = useState<ReadonlySet<number>>(new Set());
  const queryClient = useQueryClient();
  const now = today();

  const newButton = (
    <Button size="sm" onClick={() => setEditing("new")}>
      <PlusIcon className="size-4" weight="bold" aria-hidden="true" />
      New milestone
    </Button>
  );
  const display = (
    <DisplayMenu
      plain
      choices={CHOICES}
      value={displayValue}
      onChange={(next) => setQuery(displayQuery(next))}
    />
  );
  const header = (
    <ProjectHeader
      project={project}
      tab="milestones"
      actions={display}
      primary={newButton}
      onTerminal={onTerminal}
    />
  );
  if (data.isPending)
    return (
      <>
        {header}
        <SkeletonList rows={3} message="Reading the milestones" />
      </>
    );
  if (data.isError) {
    return (
      <>
        {header}
        <ReadFailed
          error={data.error}
          fallback="The milestones could not be read. Try again."
          busy={data.isFetching}
          onRetry={() => void data.refetch()}
        />
      </>
    );
  }
  const shown = sorted(inProject(data.data, project.id, madeHere), displayValue.sort);

  return (
    <>
      {header}
      {shown.length === 0 ? (
        <Empty
          title="No milestone yet"
          body="A milestone is a goal with a target date, shared by the organisation. Make one here, or put a card of this project in one from the card's properties."
          action={
            <Button size="sm" onClick={() => setEditing("new")}>
              New milestone
            </Button>
          }
        />
      ) : (
        <>
          <Timeline
            list={shown}
            project={project.id}
            now={now}
            opened={opened}
            onToggle={(milestone) =>
              setQuery({ milestone: milestone.id === opened ? "" : String(milestone.id) })
            }
            onEdit={setEditing}
            onDelete={setDeleting}
          />
        </>
      )}
      <MilestoneDialog
        editing={editing}
        onClose={() => setEditing(null)}
        onMade={(made) => setMadeHere((ids) => new Set(ids).add(made.id))}
      />
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
