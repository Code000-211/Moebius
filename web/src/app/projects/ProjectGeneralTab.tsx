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
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { Project } from "../../api/types/projects";
import { Button } from "../../ui/components/Button";
import { Input } from "../../ui/components/Input";
import { Box } from "../../ui/page/Box";
import { Field } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";
import { Switch } from "../../ui/page/Switch";

export function ProjectGeneralTab({ project, canEdit }: { project: Project; canEdit: boolean }) {
  return (
    <div className="flex flex-col gap-6">
      <Box title="Name">
        <Name key={project.name} project={project} canEdit={canEdit} />
      </Box>
      <Box title="Access">
        <Access project={project} canEdit={canEdit} />
      </Box>
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
  const changed = name.trim() !== project.name && name.trim() !== "";
  return (
    <form
      className="flex max-w-md flex-col gap-4"
      onSubmit={(event) => {
        event.preventDefault();
        if (changed && !rename.isPending) rename.mutate();
      }}
    >
      <Field label="Name" hint="Its settings move with it, in one commit.">
        <Input
          value={name}
          disabled={!canEdit}
          autoComplete="off"
          onChange={(event) => {
            rename.reset();
            setName(event.target.value);
          }}
        />
      </Field>
      <Field
        label="Key"
        hint="Names the project's cards, such as WEB-24. It stays when the project is renamed."
      >
        <Input value={project.key} readOnly mono />
      </Field>
      {rename.isError && (
        <Notice tone="error">
          <span className="font-semibold">Couldn't rename the project. </span>
          {sayError(rename.error, "Try again.")}
        </Notice>
      )}
      {canEdit && (
        <div>
          <Button type="submit" size="sm" disabled={!changed || rename.isPending}>
            {rename.isPending ? "Saving" : "Save"}
          </Button>
        </div>
      )}
    </form>
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
  return (
    <div className="flex flex-col gap-3">
      <p className="m-0 max-w-prose text-base text-secondary">
        A private project is seen only by its members: its board, cards, runs and files. An owner
        who is not a member still sees this page, to name the members.
      </p>
      {canEdit ? (
        <Switch
          label="Private"
          checked={project.private}
          disabled={change.isPending || (!project.private && me === undefined)}
          on="Private"
          off="Public"
          onChange={(next) => change.mutate(next)}
        />
      ) : (
        <p className="m-0 text-base text-text">{project.private ? "Private" : "Public"}</p>
      )}
      {change.isError && (
        <Notice tone="error">
          <span className="font-semibold">Couldn't change the access. </span>
          {sayError(change.error, "Try again.")}
        </Notice>
      )}
    </div>
  );
}
