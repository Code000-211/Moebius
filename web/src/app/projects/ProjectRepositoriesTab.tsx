/**
 * The Repositories tab of a project's settings: its repositories, each with the project its new issues land in (its
 * home project); a row opens the repository's page, where its code settings, plugin settings, rules.md, checks and
 * files are; an owner takes one out (not from its
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
import { keys } from "../../api/keys";
import type { Project } from "../../api/types/projects";
import { Button } from "../../ui/components/Button";
import { useErrorToast } from "../../ui/lib/useErrorToast";
import { Select } from "../../ui/page/Field";
import { repositoryPath } from "../pages";
import { BacklogSettings } from "./BacklogSettings";

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
  useErrorToast(remove.error ?? add.error, "Couldn't change the repositories.");
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
    <div>
      {project.repositories.length === 0 ? (
        <p className="m-0 border-t border-hairline py-3 text-base text-secondary">
          {project.files != null ? (
            <>
              No repository: this project works on its own files until it has one.{" "}
              <Link className="underline" to={repositoryPath(project.id, project.files)}>
                Project files
              </Link>
            </>
          ) : (
            "No repository yet."
          )}
        </p>
      ) : (
        <ul className="m-0 list-none border-t border-hairline p-0" aria-label="Repositories">
          {project.repositories.map((repository) => (
            <li
              key={repository.id}
              data-testid="project-repository"
              className="flex h-9 items-center gap-2.5 border-b border-hairline text-base"
            >
              <Link
                to={repositoryPath(project.id, repository.id)}
                aria-label={`Settings of ${repository.name}`}
                className="min-w-0 flex-1 truncate font-medium text-text hover:underline"
              >
                {repository.name}
              </Link>
              <span className="shrink-0 text-xs text-muted">
                {repository.home
                  ? "home project"
                  : `home: ${homeName(repository.id) ?? "another project"}`}
              </span>
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
      {canEdit && (
        <div className="flex flex-wrap items-center gap-3 py-3">
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
      {project.repositories.length > 0 && <BacklogSettings project={project} canEdit={canEdit} />}
    </div>
  );
}
