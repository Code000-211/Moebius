/**
 * The General tab of a project's settings: its name (the folder of its settings moves with it), its key (kept for good,
 * so shown and not edited), whether it is private, whether it takes requests (intake: `PUT /v1/projects/{id}/intake`,
 * and the header's Intake tab appears and goes with it at once), and the kinds of issues it offers, ticked. Making a
 * project private from a public one names the person who does it as its only member; the Members tab names the
 * others. The name is saved with its own button, the switches and ticks at once. An owner also finds the Jira import
 * below, folded away until asked for. Anyone else reads the values.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { useLocation } from "react-router";
import { setIntake } from "../../api/calls/intake";
import { listKinds, setProjectKinds } from "../../api/calls/kinds";
import { renameProject, setProjectAccess } from "../../api/calls/projects";
import { getSession } from "../../api/calls/session";
import { keys } from "../../api/keys";
import type { Project } from "../../api/types/projects";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { useErrorToast } from "../../ui/lib/useErrorToast";
import { FieldRow, Rows } from "../../ui/page/Field";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { Switch } from "../../ui/page/Switch";
import { ProjectImportTab } from "./ProjectImportTab";

export function ProjectGeneralTab({ project, canEdit }: { project: Project; canEdit: boolean }) {
  return (
    <div className="flex flex-col gap-8">
      <Rows>
        <Name key={project.name} project={project} canEdit={canEdit} />
        <FieldRow label="Key" description="Set when the project was made">
          <span className="font-mono text-base text-secondary">{project.key}</span>
        </FieldRow>
        <Access project={project} canEdit={canEdit} />
        <Intake project={project} canEdit={canEdit} />
        <Kinds project={project} canEdit={canEdit} />
      </Rows>
      {canEdit && <Import project={project} />}
    </div>
  );
}

function Name({ project, canEdit }: { project: Project; canEdit: boolean }) {
  const queryClient = useQueryClient();
  const [name, setName] = useState(project.name);
  const rename = useMutation({
    mutationFn: () => renameProject(project.id, name.trim()),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: keys.projects }),
  });
  useErrorToast(rename.error, "Couldn't rename the project.");
  const changed = name.trim() !== project.name && name.trim() !== "";
  return (
    <FieldRow label="Name" description="Its settings move with it, in one commit.">
      <form
        className="flex items-center gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          if (changed && !rename.isPending) rename.mutate();
        }}
      >
        <Input
          className="w-64"
          value={name}
          disabled={!canEdit}
          autoComplete="off"
          aria-label="Name"
          onChange={(event) => {
            rename.reset();
            setName(event.target.value);
          }}
        />
        {canEdit && (
          <Button type="submit" size="sm" disabled={!changed || rename.isPending}>
            {rename.isPending ? "Saving" : "Save"}
          </Button>
        )}
      </form>
    </FieldRow>
  );
}

function Access({ project, canEdit }: { project: Project; canEdit: boolean }) {
  const queryClient = useQueryClient();
  const session = useQuery({ queryKey: keys.session, queryFn: getSession, enabled: false });
  const me = session.data?.person?.handle;
  const change = useMutation({
    mutationFn: (makePrivate: boolean) =>
      setProjectAccess(project.id, {
        private: makePrivate,
        // A public project lists everyone: the person who makes it private starts it with themselves alone.
        members: project.private ? project.members.map((m) => m.handle) : me ? [me] : [],
      }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: keys.projects }),
  });
  useErrorToast(change.error, "Couldn't change the access.");
  return (
    <FieldRow
      label="Private"
      description="Seen only by its members: its board, cards, runs and files."
    >
      {canEdit ? (
        <Switch
          label="Private"
          checked={project.private}
          disabled={change.isPending || (!project.private && me === undefined)}
          onChange={(next) => change.mutate(next)}
        />
      ) : (
        <span className="text-base text-text">{project.private ? "Private" : "Public"}</span>
      )}
    </FieldRow>
  );
}

/** Replaces the project in the cached list with what the server answered, so the header changes at once. */
function useProjectCache() {
  const queryClient = useQueryClient();
  return (changed: Project) => {
    queryClient.setQueryData<Project[]>(keys.projects, (all) =>
      all?.map((each) => (each.id === changed.id ? changed : each)),
    );
    void queryClient.invalidateQueries({ queryKey: keys.projects });
  };
}

function Intake({ project, canEdit }: { project: Project; canEdit: boolean }) {
  const cache = useProjectCache();
  const change = useMutation({
    mutationFn: (enabled: boolean) => setIntake(project.id, enabled),
    onSuccess: cache,
  });
  useErrorToast(change.error, "Couldn't change intake.");
  return (
    <FieldRow label="Intake" description="People outside can send requests">
      {canEdit ? (
        <Switch
          label="Intake"
          checked={project.intake}
          disabled={change.isPending}
          onChange={(next) => change.mutate(next)}
        />
      ) : (
        <span className="text-base text-text">{project.intake ? "On" : "Off"}</span>
      )}
    </FieldRow>
  );
}

/** The kinds of the organisation, ticked for the ones the project offers; each tick is saved at once. */
function Kinds({ project, canEdit }: { project: Project; canEdit: boolean }) {
  const queryClient = useQueryClient();
  const kinds = useQuery({ queryKey: keys.issueKinds, queryFn: listKinds });
  const offer = useMutation({
    mutationFn: (names: string[]) => setProjectKinds(project.id, names),
    onSuccess: () =>
      Promise.all([
        queryClient.invalidateQueries({ queryKey: keys.projects }),
        queryClient.invalidateQueries({ queryKey: keys.issueKinds }),
      ]),
  });
  useErrorToast(offer.error, "Couldn't change the kinds.");
  const offered = (kind: { projects: number[] }) => kind.projects.includes(project.id);
  return (
    <FieldRow label="Kinds of issues" description="Which kinds this project offers">
      {kinds.isPending ? (
        <SkeletonLines lines={1} message="Reading the kinds" />
      ) : kinds.isError ? (
        <span className="text-base text-secondary">The kinds could not be read.</span>
      ) : kinds.data.length === 0 ? (
        <span className="text-base text-secondary">No kind has been made yet.</span>
      ) : (
        <fieldset className="m-0 flex max-w-md flex-wrap justify-end gap-x-4 gap-y-1 border-0 p-0">
          <legend className="sr-only">Kinds this project offers</legend>
          {kinds.data.map((kind) => (
            <label key={kind.id} className="flex items-center gap-2 text-base">
              <input
                type="checkbox"
                checked={offered(kind)}
                disabled={!canEdit || offer.isPending}
                onChange={(event) =>
                  offer.mutate(
                    kinds.data
                      .filter((each) =>
                        each.id === kind.id ? event.target.checked : offered(each),
                      )
                      .map((each) => each.name),
                  )
                }
              />
              {kind.name}
            </label>
          ))}
        </fieldset>
      )}
    </FieldRow>
  );
}

/** The Jira import, folded away until asked for (the header's menu opens General with `#import`). */
function Import({ project }: { project: Project }) {
  const [open, setOpen] = useState(useLocation().hash === "#import");
  return (
    <div className="flex flex-col gap-3">
      <div>
        <Button
          size="sm"
          variant="secondary"
          aria-expanded={open}
          onClick={() => setOpen((was) => !was)}
        >
          {open ? "Hide the Jira import" : "Import from Jira"}
        </Button>
      </div>
      {open && <ProjectImportTab project={project} canEdit />}
    </div>
  );
}
