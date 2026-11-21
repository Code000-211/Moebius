/**
 * The Milestones page (pattern 15 of the design drafts): the goals of the organisation, each a row on a time axis
 * with its target date and how far its cards are. A milestone holds cards of any projects; the
 * server counts and lists only the cards of the projects the person may see, so every number here is theirs. A row opens
 * to its description, its share in each project and its cards; a card is put in a milestone from its properties.
 */
import { PlusIcon } from "@phosphor-icons/react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo, useState } from "react";
import { deleteMilestone, listMilestones } from "../../api/calls/milestones";
import { listProjects } from "../../api/calls/projects";
import { keys } from "../../api/keys";
import type { MilestoneView } from "../../api/types/milestones";
import { Button } from "../../ui/components/Button";
import { FilterBar } from "../../ui/filters/FilterBar";
import { displayQuery, readDisplay } from "../../ui/filters/model";
import { useQueryParams } from "../../ui/filters/useQueryParams";
import { ConfirmDelete } from "../../ui/page/ConfirmDelete";
import { Empty } from "../../ui/page/Empty";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonList } from "../../ui/page/Skeleton";
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

export function MilestonesPage() {
  const data = useQuery({ queryKey: keys.milestones, queryFn: listMilestones });
  const projects = useQuery({ queryKey: keys.projects, queryFn: listProjects });
  const [params, setQuery] = useQueryParams();
  const display = useMemo(() => readDisplay(params, CHOICES), [params]);
  const project = params.get("project") ?? "";
  // The milestone that is opened is in the address, so a reload and a link keep it open.
  const opened = Number(params.get("milestone")) || null;
  const [editing, setEditing] = useState<MilestoneView | "new" | null>(null);
  const [deleting, setDeleting] = useState<MilestoneView | null>(null);
  const queryClient = useQueryClient();
  const now = today();

  const header = (
    <PageHeader
      title="Milestones"
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
              value: display,
              onChange: (next) => setQuery(displayQuery(next)),
            }}
          />
          {shown.length === 0 ? (
            <Empty title="No milestone matches" body="No milestone holds a card of that project." />
          ) : (
            <Timeline
              list={shown}
              now={now}
              opened={opened}
              onToggle={(milestone) =>
                setQuery({ milestone: milestone.id === opened ? "" : String(milestone.id) })
              }
              onEdit={setEditing}
              onDelete={setDeleting}
            />
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
