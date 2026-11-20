/**
 * The Repositories entry of Settings: the repositories of every project, each leading to its own page, and the way to
 * add one. It reads the projects the Projects page reads (`projects/`), and adds nothing the server lacks.
 */
import { PlusIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router";
import { listProjects } from "../../api/calls/projects";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { FieldRow, Rows } from "../../ui/page/Field";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonList } from "../../ui/page/Skeleton";
import { AreaColumn, AreaTitle, Section } from "./parts";

export function RepositoriesPage() {
  const data = useQuery({ queryKey: keys.projects, queryFn: listProjects });
  const withRepositories = (data.data ?? []).filter((project) => project.repositories.length > 0);
  return (
    <AreaColumn>
      <AreaTitle
        title="Repositories"
        actions={
          <Button asChild size="sm">
            <Link to="/projects/new">
              <PlusIcon className="size-4" weight="bold" aria-hidden="true" />
              Add a repository
            </Link>
          </Button>
        }
      />
      {data.isError ? (
        <ReadFailed
          error={data.error}
          fallback="Could not read the repositories."
          onRetry={() => void data.refetch()}
        />
      ) : data.isPending ? (
        <SkeletonList rows={3} />
      ) : withRepositories.length === 0 ? (
        <p className="m-0 text-base text-secondary">No repository yet.</p>
      ) : (
        withRepositories.map((project) => {
          const rows = (
            <Rows key={project.id}>
              {project.repositories.map((repository) => (
                <FieldRow
                  key={repository.id}
                  label={repository.name}
                  description={repository.home ? "Home project" : undefined}
                >
                  <Button asChild size="sm" variant="secondary">
                    <Link
                      to={`/projects/${repository.id}`}
                      aria-label={`Settings of ${repository.name}`}
                    >
                      Open
                    </Link>
                  </Button>
                </FieldRow>
              ))}
            </Rows>
          );
          // The project is named only where there is more than one to tell apart.
          return withRepositories.length > 1 ? (
            <Section key={project.id} title={project.name}>
              {rows}
            </Section>
          ) : (
            <div key={project.id}>{rows}</div>
          );
        })
      )}
    </AreaColumn>
  );
}
