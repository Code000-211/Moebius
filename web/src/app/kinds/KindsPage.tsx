/**
 * The Kinds page, for owners: the kinds of issues of the organisation (a bug, a feature, a task), each with fields of
 * its own (text, number, choice, date, person), and which projects offer which kind. A kind is edited as a whole and
 * saved with its own button; a project's choice is saved at once. What the server refuses (a name twice, a choice
 * without options, a field whose type changed, a kind that cards still have) is said where it was asked. Each kind
 * also holds its issue template (`IssueTemplate.tsx`).
 */
import { PlusIcon, TrashIcon } from "@phosphor-icons/react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { addKind, deleteKind, listKinds, setProjectKinds, updateKind } from "../../api/calls/kinds";
import { listProjects } from "../../api/calls/projects";
import { keys } from "../../api/keys";
import { FIELD_TYPES, type Kind } from "../../api/types/kinds";
import type { Project } from "../../api/types/projects";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { useErrorToast } from "../../ui/lib/useErrorToast";
import { Box } from "../../ui/page/Box";
import { Empty } from "../../ui/page/Empty";
import { FieldRow, Rows, Select } from "../../ui/page/Field";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { AreaTitle } from "../settings/parts";
import { NotFoundPage } from "../shell/Page";
import { useRole } from "../shell/session";
import { IssueTemplateBox } from "./IssueTemplate";
import { bodyOf, draftOf, type FieldDraft, type KindDraft } from "./model";

export function KindsPage() {
  const role = useRole();
  if (role !== undefined && role !== "owner") return <NotFoundPage />;
  return <Kinds />;
}

function Kinds() {
  const kinds = useQuery({ queryKey: keys.issueKinds, queryFn: listKinds });
  const projects = useQuery({ queryKey: keys.projects, queryFn: listProjects });
  return (
    <div data-testid="kinds-page" className="flex max-w-3xl flex-col gap-6">
      <AreaTitle title="Kinds of issues" />
      <KindForm title="New kind" />
      {kinds.isPending && <SkeletonLines lines={3} message="Reading the kinds" />}
      {kinds.isError && (
        <ReadFailed
          error={kinds.error}
          fallback="Could not read the kinds. Try again."
          busy={kinds.isFetching}
          onRetry={() => void kinds.refetch()}
        />
      )}
      {kinds.data?.length === 0 && (
        <Empty title="No kind yet" body="Make one above, then let a project pick it." />
      )}
      {kinds.data?.map((kind) => (
        <KindBox key={kind.id} kind={kind} kinds={kinds.data} projects={projects.data ?? []} />
      ))}
    </div>
  );
}

/** A kind: its form and the projects that offer it. */
function KindBox({ kind, kinds, projects }: { kind: Kind; kinds: Kind[]; projects: Project[] }) {
  const queryClient = useQueryClient();
  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: keys.issueKinds });
    void queryClient.invalidateQueries({ queryKey: keys.projects });
  };
  const remove = useMutation({ mutationFn: () => deleteKind(kind.id), onSuccess: refresh });
  useErrorToast(remove.error, "Could not delete the kind.");
  return (
    <Box
      testId={`kind-${kind.name}`}
      title={kind.name}
      actions={
        <Button
          variant="ghost"
          size="sm"
          aria-label={`Delete the kind ${kind.name}`}
          onClick={() => remove.mutate()}
        >
          <TrashIcon className="size-4" aria-hidden="true" />
          Delete
        </Button>
      }
    >
      <div className="flex flex-col gap-4">
        <OfferedBy kind={kind} kinds={kinds} projects={projects} onSaved={refresh} />
        <KindForm kind={kind} />
        <IssueTemplateBox kind={kind} />
      </div>
    </Box>
  );
}

/** The names of the kinds `project` offers once `kind` is offered by it (`on`) or is not. */
export function offeredNames(
  kinds: readonly Kind[],
  project: Pick<Project, "id">,
  kind: Kind,
  on: boolean,
): string[] {
  return kinds
    .filter((k) => (k.id === kind.id ? on : k.projects.includes(project.id)))
    .map((k) => k.name);
}

/** The projects, each with whether it offers the kind. */
function OfferedBy({
  kind,
  kinds,
  projects,
  onSaved,
}: {
  kind: Kind;
  kinds: Kind[];
  projects: Project[];
  onSaved: () => void;
}) {
  const offer = useMutation({
    mutationFn: ({ project, on }: { project: Project; on: boolean }) =>
      setProjectKinds(project.id, offeredNames(kinds, project, kind, on)),
    onSuccess: onSaved,
  });
  useErrorToast(offer.error, "Could not change what the project offers.");
  return (
    <fieldset className="m-0 border-0 p-0">
      <legend className="mb-1 p-0 text-sm font-medium text-secondary">Offered by</legend>
      {projects.map((project) => (
        <label
          key={project.id}
          className="flex h-9 items-center gap-2 border-b border-hairline text-base first:border-t"
        >
          <input
            type="checkbox"
            // The tick shows at once; the list it is saved into is read again after.
            checked={
              offer.isPending && offer.variables.project.id === project.id
                ? offer.variables.on
                : kind.projects.includes(project.id)
            }
            disabled={offer.isPending}
            onChange={(event) => offer.mutate({ project, on: event.target.checked })}
          />
          {project.name}
        </label>
      ))}
    </fieldset>
  );
}

/** The form of a kind: its name and its fields. A kind that is given is saved over; without one a new kind is made. */
function KindForm({ kind, title }: { kind?: Kind; title?: string }) {
  const queryClient = useQueryClient();
  const [draft, setDraft] = useState<KindDraft>(() => draftOf(kind));
  const save = useMutation({
    mutationFn: () => (kind ? updateKind(kind.id, bodyOf(draft)) : addKind(bodyOf(draft))),
    onSuccess: async () => {
      if (!kind) setDraft(draftOf());
      await queryClient.invalidateQueries({ queryKey: keys.issueKinds });
    },
  });
  useErrorToast(save.error, "Could not save the kind.");
  const unsaved = JSON.stringify(draft) !== JSON.stringify(draftOf(kind));
  const field = (index: number, change: Partial<FieldDraft>) =>
    setDraft({
      ...draft,
      fields: draft.fields.map((f, i) => (i === index ? { ...f, ...change } : f)),
    });
  const body = (
    <form
      className="flex flex-col gap-3"
      onSubmit={(event) => {
        event.preventDefault();
        save.mutate();
      }}
    >
      <Rows>
        <FieldRow wide label="Name">
          <Input
            value={draft.name}
            onChange={(event) => setDraft({ ...draft, name: event.target.value })}
          />
        </FieldRow>
      </Rows>
      {draft.fields.map((f, index) => (
        // The fields have no id of their own in the form; a field is told by its place.
        // biome-ignore lint/suspicious/noArrayIndexKey: the form's fields are edited by place
        <div key={index} className="grid gap-2 sm:grid-cols-[1fr_8rem_2fr_auto]">
          <Input
            aria-label={`Name of field ${index + 1}`}
            value={f.name}
            onChange={(event) => field(index, { name: event.target.value })}
          />
          <Select
            aria-label={`Type of field ${index + 1}`}
            options={FIELD_TYPES}
            value={f.type}
            onChange={(type) => field(index, { type: type as FieldDraft["type"] })}
          />
          {f.type === "choice" ? (
            <Input
              aria-label={`Options of field ${index + 1}`}
              placeholder="Options, separated by commas"
              value={f.options}
              onChange={(event) => field(index, { options: event.target.value })}
            />
          ) : (
            <span />
          )}
          <Button
            variant="ghost"
            size="sm"
            aria-label={`Remove field ${index + 1}`}
            onClick={() =>
              setDraft({ ...draft, fields: draft.fields.filter((_, i) => i !== index) })
            }
          >
            <TrashIcon className="size-4" aria-hidden="true" />
          </Button>
        </div>
      ))}
      <div className="flex gap-2">
        <Button
          variant="secondary"
          size="sm"
          onClick={() =>
            setDraft({
              ...draft,
              fields: [...draft.fields, { name: "", type: "text", options: "" }],
            })
          }
        >
          <PlusIcon className="size-4" aria-hidden="true" />
          Add a field
        </Button>
        <Button type="submit" size="sm" disabled={draft.name.trim() === "" || save.isPending}>
          {kind ? "Save kind" : "Add kind"}
        </Button>
        {unsaved && kind && (
          <span role="status" className="self-center text-sm text-secondary">
            Unsaved changes
          </span>
        )}
      </div>
    </form>
  );
  return title ? <Box title={title}>{body}</Box> : body;
}
