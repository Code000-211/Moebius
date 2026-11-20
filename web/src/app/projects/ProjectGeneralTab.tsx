/**
 * The General tab of a project's settings: its name (the folder of its settings moves with it), its key (kept for good,
 * so shown and not edited), and whether it is private. Making a project private from a public one names the person
 * who does it as its only member; the Members tab names the others. The name is saved with its own button, the switch
 * at once.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { renameProject, setProjectAccess } from "../../api/calls/projects";
import { getSession } from "../../api/calls/session";
import { keys } from "../../api/keys";
import type { Project } from "../../api/types/projects";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { useErrorToast } from "../../ui/lib/useErrorToast";
import { FieldRow, Rows } from "../../ui/page/Field";
import { Switch } from "../../ui/page/Switch";
import { Code, ValueRow } from "../settings/parts";

export function ProjectGeneralTab({ project, canEdit }: { project: Project; canEdit: boolean }) {
  return (
    <Rows>
      <Name key={project.name} project={project} canEdit={canEdit} />
      <ValueRow label="Key" value={<Code>{project.key}</Code>} />
      <Access project={project} canEdit={canEdit} />
    </Rows>
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
