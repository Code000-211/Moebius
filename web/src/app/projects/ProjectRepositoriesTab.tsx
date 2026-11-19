/**
 * The Repositories tab of a project's settings: its repositories, each with the project its new issues land in (its
 * home project) and a link to the repository's page, where its code settings are; an owner takes one out (not from its
 * home project, which keeps it until the repository is removed) and puts another of the server's repositories in.
 * "Clone a new repository" is the Add a repository page, which makes the repository's own home project.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link } from "react-router";
import { snapshot as readSnapshot } from "../../api/calls/board";
import {
  addProjectRepository,
  listProjects,
  removeProjectRepository,
} from "../../api/calls/projects";
import { sayError } from "../../api/errorText";
import { keys } from "../../api/keys";
import type { Project } from "../../api/types/projects";
import { Button } from "../../ui/components/Button";
import { Box } from "../../ui/page/Box";
import { Select } from "../../ui/page/Field";
import { Notice } from "../../ui/page/Notice";

export function ProjectRepositoriesTab({
  project,
  canEdit,
}: {
  project: Project;
  canEdit: boolean;
}) {
  const queryClient = useQueryClient();
  const snapshot = useQuery({ queryKey: keys.snapshot, queryFn: () => readSnapshot() });
  const projects = useQuery({ queryKey: keys.projects, queryFn: listProjects });
  const [picked, setPicked] = useState("");
  const done = () => {
    void queryClient.invalidateQueries({ queryKey: keys.projects });
    void queryClient.invalidateQueries({ queryKey: keys.snapshot });
  };
  const add = useMutation({
    mutationFn: (repository: number) => addProjectRepository(project.id, repository),
    onSuccess: () => {
      setPicked("");
      done();
    },
  });
  const remove = useMutation({
    mutationFn: (repository: number) => removeProjectRepository(project.id, repository),
    onSuccess: done,
  });
  const repositories = (snapshot.data?.boards ?? [])
    .map((board) => board.repository)
    // The repositories that hold a project's own files are no one else's to share.
    .filter((repository) => repository.files_of == null);
  const offered = repositories.filter(
    (repository) => !project.repositories.some((each) => each.id === repository.id),
  );
  const homeName = (repository: number) => {
    const home = repositories.find((each) => each.id === repository)?.home_project_id;
    return projects.data?.find((each) => each.id === home)?.name;
  };
  return (
    <Box title="Repositories" padded={false}>
      {project.repositories.length === 0 ? (
        <p className="m-0 p-4 text-base text-secondary">
          {project.files != null ? (
            <>
              No repository: this project works on its own files until it has one.{" "}
              <Link className="underline" to={`/projects/${project.files}`}>
                Project files
              </Link>
            </>
          ) : (
            "No repository yet."
          )}
        </p>
      ) : (
        <ul className="m-0 list-none p-0" aria-label="Repositories">
          {project.repositories.map((repository) => (
            <li
              key={repository.id}
              data-testid="project-repository"
              className="flex min-h-11 flex-wrap items-center gap-3 px-4 py-2"
            >
              <span className="min-w-0 flex-1 truncate text-base font-semibold text-text">
                {repository.name}
              </span>
              <span className="text-sm text-secondary">
                {repository.home
                  ? "home project"
                  : `home: ${homeName(repository.id) ?? "another project"}`}
              </span>
              <Button asChild size="sm" variant="secondary">
                <Link
                  to={`/projects/${repository.id}`}
                  aria-label={`Settings of ${repository.name}`}
                >
                  Code settings
                </Link>
              </Button>
              {canEdit && !repository.home && (
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={remove.isPending}
                  aria-label={`Take ${repository.name} out`}
                  onClick={() => remove.mutate(repository.id)}
                >
                  Take out
                </Button>
              )}
            </li>
          ))}
        </ul>
      )}
      {(remove.isError || add.isError) && (
        <div className="px-4 pb-4">
          <Notice tone="error">
            <span className="font-semibold">Couldn't change the repositories. </span>
            {sayError(remove.error ?? add.error, "Try again.")}
          </Notice>
        </div>
      )}
      {canEdit && (
        <div className="flex flex-wrap items-center gap-3 border-t border-hairline p-4">
          {offered.length > 0 && (
            <>
              <Select
                aria-label="Repository to add"
                className="w-56"
                options={[
                  ["", "Choose a repository"],
                  ...offered.map((r): [string, string] => [String(r.id), r.name]),
                ]}
                value={picked}
                onChange={setPicked}
              />
              <Button
                size="sm"
                disabled={picked === "" || add.isPending}
                onClick={() => add.mutate(Number(picked))}
              >
                Add to project
              </Button>
            </>
          )}
          <Button asChild size="sm" variant="secondary">
            <Link to="/projects/new">Clone a new repository</Link>
          </Button>
        </div>
      )}
    </Box>
  );
}
