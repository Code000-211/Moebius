/**
 * `/projects/:projectId/workflows|cycles|milestones|intake` (and `cycles/:cycleId`): a tab of a project. Workflows,
 * Cycles, Milestones and Intake are the tabs' own pages, each drawing the project's header.
 * An id that is no project says so, and so does the Intake address of a project that takes no requests.
 */
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Link, useParams } from "react-router";
import { listProjects } from "../../api/calls/projects";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { Empty } from "../../ui/page/Empty";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonCard } from "../../ui/page/Skeleton";
import { RepositoryTerminal } from "../board/RepositoryTerminal";
import { CyclesTab } from "../cycles/CyclesTab";
import { IntakeTab } from "../intake/IntakeTab";
import { MilestonesTab } from "../milestones/MilestonesTab";
import type { ProjectTab } from "../pages";
import { WorkflowsTab } from "../workflows/WorkflowsTab";
import { boardRepositoryOf } from "./model";
import { ProjectHeader } from "./ProjectHeader";

export function ProjectTabRoute({ tab }: { tab: ProjectTab }) {
  const { projectId, cycleId } = useParams();
  const [terminalOpen, setTerminalOpen] = useState(false);
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
  if (!projects.data)
    return (
      <>
        <PageHeader title="Project" />
        <SkeletonCard />
      </>
    );
  const project = projects.data.find((each) => String(each.id) === projectId);
  if (!project)
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
  // A project that takes no requests has no Intake tab, so its address is no page either.
  const off = tab === "intake" && !project.intake;
  const repository = boardRepositoryOf(project);
  const onTerminal = () => setTerminalOpen(true);
  return (
    <>
      {off ? (
        <>
          <ProjectHeader project={project} tab="board" onTerminal={onTerminal} />
          <Empty
            title="Intake is off"
            body="This project takes no requests. An owner turns intake on in the project's settings."
          />
        </>
      ) : tab === "cycles" ? (
        <CyclesTab
          key={project.id}
          project={project}
          cycleId={cycleId === undefined ? null : Number(cycleId)}
          onTerminal={onTerminal}
        />
      ) : tab === "milestones" ? (
        <MilestonesTab key={project.id} project={project} onTerminal={onTerminal} />
      ) : tab === "intake" ? (
        <IntakeTab key={project.id} project={project} onTerminal={onTerminal} />
      ) : (
        <WorkflowsTab key={project.id} project={project} onTerminal={onTerminal} />
      )}
      {repository !== null && (
        <RepositoryTerminal
          repository={repository}
          open={terminalOpen}
          onOpenChange={setTerminalOpen}
        />
      )}
    </>
  );
}
