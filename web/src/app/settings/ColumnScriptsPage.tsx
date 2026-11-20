/**
 * The Column scripts entry of Settings: the script each column of a project's board runs, the project's own or the
 * built-in one, each leading to the editor of the board (`/board/:id/scripts/:column`). It adds no route: the lists are
 * `GET /v1/projects` and `GET /v1/projects/{id}/scripts`.
 */
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router";
import { projectScripts } from "../../api/calls/board";
import { listProjects } from "../../api/calls/projects";
import { keys } from "../../api/keys";
import type { Project } from "../../api/types/projects";
import { Button } from "../../ui/components/Button";
import { FieldRow, Rows } from "../../ui/page/Field";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonList } from "../../ui/page/Skeleton";
import { AreaColumn, AreaTitle, Section } from "./parts";

/** The rows of one project's board: a read of its own, so the others do not wait for it. */
function ProjectScripts({ project }: { project: Project }) {
  const repository = project.repositories.find((r) => r.home) ?? project.repositories[0];
  const read = useQuery({
    queryKey: keys.projectScripts(project.id),
    queryFn: () => projectScripts(project.id),
  });
  if (read.isError) {
    return (
      <ReadFailed
        error={read.error}
        fallback="Could not read the scripts."
        onRetry={() => void read.refetch()}
      />
    );
  }
  if (!read.data) return <SkeletonList rows={3} />;
  return (
    <Rows>
      {read.data.map((each) => (
        <FieldRow
          key={each.column}
          label={each.column}
          description={each.source === "project" ? "This project's own" : "Built in"}
        >
          <Button asChild size="sm" variant="secondary">
            <Link
              to={`/board/${repository?.id}/scripts/${encodeURIComponent(each.column)}?project=${project.id}`}
              aria-label={`Script of ${each.column}`}
            >
              Open
            </Link>
          </Button>
        </FieldRow>
      ))}
    </Rows>
  );
}

export function ColumnScriptsPage() {
  const projects = useQuery({ queryKey: keys.projects, queryFn: listProjects });
  const withBoard = (projects.data ?? []).filter((project) => project.repositories.length > 0);
  return (
    <AreaColumn>
      <AreaTitle title="Column scripts" />
      {projects.isError ? (
        <ReadFailed
          error={projects.error}
          fallback="Could not read the projects."
          onRetry={() => void projects.refetch()}
        />
      ) : projects.isPending ? (
        <SkeletonList rows={4} />
      ) : withBoard.length === 0 ? (
        <p className="m-0 text-base text-secondary">No board yet.</p>
      ) : withBoard.length > 1 ? (
        withBoard.map((project) => (
          <Section key={project.id} title={project.name}>
            <ProjectScripts project={project} />
          </Section>
        ))
      ) : (
        withBoard[0] && <ProjectScripts project={withBoard[0]} />
      )}
    </AreaColumn>
  );
}
