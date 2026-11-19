/**
 * The Members tab of a project's settings. A public project is everyone's, so the tab lists them and says how to
 * name members; a private one lists the people of the organisation with the members ticked, which its owner edits
 * and saves (`PUT /v1/projects/{id}/access`). Anyone else reads the list. An owner who unticks themselves keeps the
 * project's record but loses its board, which the tab says before the save.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link } from "react-router";
import { people as loadPeople } from "../../api/calls/people";
import { setProjectAccess } from "../../api/calls/projects";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { Project } from "../../api/types/projects";
import { Button } from "../../ui/components/Button";
import { Box } from "../../ui/page/Box";
import { Notice } from "../../ui/page/Notice";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonLines } from "../../ui/page/Skeleton";
import { assignable } from "../people/AssigneesProperty";

export function ProjectMembersTab({ project, canEdit }: { project: Project; canEdit: boolean }) {
  const list = (
    <ul className="m-0 flex list-none flex-col gap-1 p-0" aria-label="Members">
      {project.members.map((member) => (
        <li key={member.id} className="text-base text-text">
          {member.name === "" ? `@${member.handle}` : `${member.name} (@${member.handle})`}
        </li>
      ))}
    </ul>
  );
  if (!project.private)
    return (
      <Box title="Members">
        <p className="m-0 mb-3 max-w-prose text-base text-secondary">
          This project is public: everyone of the organisation who is not disabled is a member.{" "}
          {canEdit && (
            <>
              Make it private on the{" "}
              <Link className="underline" to={`/projects/${project.id}/settings`}>
                General tab
              </Link>{" "}
              to name its members.
            </>
          )}
        </p>
        {list}
      </Box>
    );
  if (!canEdit) return <Box title="Members">{list}</Box>;
  return <Editor key={project.members.map((m) => m.handle).join(",")} project={project} />;
}

/** The people of the organisation, the members ticked, with Save. */
function Editor({ project }: { project: Project }) {
  const queryClient = useQueryClient();
  const people = useQuery({ queryKey: keys.people, queryFn: loadPeople });
  const [chosen, setChosen] = useState(() => new Set(project.members.map((m) => m.handle)));
  const save = useMutation({
    mutationFn: () => setProjectAccess(project.id, { private: true, members: [...chosen] }),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: keys.projects }),
  });
  if (people.isPending)
    return (
      <Box title="Members">
        <SkeletonLines lines={3} message="Reading the people" />
      </Box>
    );
  if (people.isError)
    return (
      <Box title="Members">
        <ReadFailed
          error={people.error}
          fallback="The people could not be read. Try again."
          busy={people.isFetching}
          onRetry={() => void people.refetch()}
        />
      </Box>
    );
  const offered = assignable(people.data);
  const was = new Set(project.members.map((m) => m.handle));
  const changed = chosen.size !== was.size || [...chosen].some((handle) => !was.has(handle));
  return (
    <Box title="Members">
      <form
        className="flex flex-col gap-4"
        onSubmit={(event) => {
          event.preventDefault();
          if (changed && !save.isPending) save.mutate();
        }}
      >
        <p className="m-0 max-w-prose text-base text-secondary">
          Only the people ticked see this project. An owner who is not ticked still sees the project
          here, but not its board.
        </p>
        <fieldset className="m-0 flex flex-col gap-1 border-0 p-0">
          <legend className="sr-only">Members</legend>
          {offered.map(([handle, label]) => (
            <label key={handle} className="flex min-h-7 items-center gap-2 text-base">
              <input
                type="checkbox"
                checked={chosen.has(handle)}
                onChange={(event) => {
                  save.reset();
                  const next = new Set(chosen);
                  if (event.target.checked) next.add(handle);
                  else next.delete(handle);
                  setChosen(next);
                }}
              />
              {label}
            </label>
          ))}
        </fieldset>
        {save.isError && (
          <Notice tone="error">
            <span className="font-semibold">Couldn't save the members. </span>
            {sayError(save.error, "Try again.")}
          </Notice>
        )}
        <div>
          <Button type="submit" size="sm" disabled={!changed || save.isPending}>
            {save.isPending ? "Saving" : "Save members"}
          </Button>
        </div>
      </form>
    </Box>
  );
}
