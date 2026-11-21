/**
 * `/projects/:projectId` and `/projects/:projectId/list`: the board of a project. Before the repository page moved
 * under its project this address was the repository's, so an id that is no project but is a repository still opens
 * that repository's page, at its home project (`oldRepositoryPage` of the legacy table).
 */
import { useQuery } from "@tanstack/react-query";
import { Link, Navigate, useLocation, useParams } from "react-router";
import { listProjects } from "../../api/calls/projects";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { Empty } from "../../ui/page/Empty";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonBoard } from "../../ui/page/Skeleton";
import { BoardPage } from "../board/BoardPage";
import { oldRepositoryPage } from "./redirects";

export function ProjectBoardRoute({ list = false }: { list?: boolean }) {
  const { projectId } = useParams();
  const { search } = useLocation();
  const projects = useQuery({ queryKey: keys.projects, queryFn: listProjects });
  if (projects.isError)
    return (
      <>
        <PageHeader title="Project" />
        <ReadFailed
          error={projects.error}
          fallback="The project could not be read. Try again."
          busy={projects.isFetching}
          onRetry={() => void projects.refetch()}
        />
      </>
    );
  if (!projects.data) return <SkeletonBoard />;
  const project = projects.data.find((each) => String(each.id) === projectId);
  if (project) return <BoardPage project={project} list={list} />;
  const old = list
    ? null
    : oldRepositoryPage(projectId, projects.data, new URLSearchParams(search));
  if (old !== null) return <Navigate to={old} replace />;
  return (
    <>
      <PageHeader title="Project" />
      <Empty
        title="No such project"
        body="This server has no project with that address. It may have been removed, or you may not see it."
        action={
          <Button asChild size="sm">
            <Link to="/projects">All projects</Link>
          </Button>
        }
      />
    </>
  );
}
