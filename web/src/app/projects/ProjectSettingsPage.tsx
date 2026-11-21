/**
 * A project's settings in tabs (pattern 21 of the design drafts; `/projects/:projectId/settings/:tab`):
 * General (its name, its key, and whether it is private), Repositories (with their home project and the settings of
 * their code), Members, Workflow (the script of each column), Kinds, Plugins and Import (a Jira project copied in, for an owner). Every setting of a project is edited
 * and saved in its tab; the tile of the Projects page opens the page. Only an owner edits: anyone else who sees the
 * project reads it. The tab is part of the address, so a tab can be linked to.
 */
import { ArrowLeftIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { useId } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { listProjects } from "../../api/calls/projects";
import { keys } from "../../api/keys";
import type { Project } from "../../api/types/projects";
import { Button } from "../../ui/components/Button";
import { Tooltip } from "../../ui/components/Tooltip";
import { Empty } from "../../ui/page/Empty";
import { PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonCard } from "../../ui/page/Skeleton";
import { TabPanel, Tabs } from "../../ui/page/Tabs";
import { boardPath, settingsPath } from "../pages";
import { useRole } from "../shell/session";
import { ProjectGeneralTab } from "./ProjectGeneralTab";
import { ProjectImportTab } from "./ProjectImportTab";
import { ProjectKindsTab } from "./ProjectKindsTab";
import { ProjectMembersTab } from "./ProjectMembersTab";
import { ProjectPluginsTab } from "./ProjectPluginsTab";
import { ProjectRepositoriesTab } from "./ProjectRepositoriesTab";
import { ProjectWorkflowTab } from "./ProjectWorkflowTab";

const TABS = [
  "general",
  "repositories",
  "members",
  "workflow",
  "kinds",
  "plugins",
  "import",
] as const;
type Tab = (typeof TABS)[number];

/** The tabs with their words; Repositories and Members say how many they hold. */
function tabLabels(project: Project): Array<{ value: Tab; label: string }> {
  return [
    { value: "general", label: "General" },
    { value: "repositories", label: `Repositories ${project.repositories.length}` },
    { value: "members", label: `Members ${project.members.length}` },
    { value: "workflow", label: "Workflow" },
    { value: "kinds", label: "Kinds" },
    { value: "plugins", label: "Plugins" },
    { value: "import", label: "Import" },
  ];
}

export function ProjectSettingsPage() {
  const { projectId: id, tab } = useParams();
  const navigate = useNavigate();
  const idBase = useId();
  const role = useRole();
  const projects = useQuery({ queryKey: keys.projects, queryFn: listProjects });
  if (projects.isPending)
    return (
      <>
        <PageHeader title="Project" />
        <SkeletonCard lines={4} message="Reading the project" />
      </>
    );
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
  const project = projects.data.find((each) => String(each.id) === id);
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
  const current: Tab = TABS.find((each) => each === tab) ?? "general";
  const canEdit = role === undefined || role === "owner";
  const board = project.repositories[0]?.id ?? project.files;
  return (
    <>
      <PageHeader
        title={project.name}
        back={
          <Tooltip content="All projects" side="bottom">
            <Button asChild variant="icon" aria-label="All projects">
              <Link to="/projects">
                <ArrowLeftIcon className="size-5" />
              </Link>
            </Button>
          </Tooltip>
        }
        tabs={
          <Tabs
            label="Project settings"
            idBase={idBase}
            value={current}
            tabs={tabLabels(project)}
            onChange={(next) => navigate(settingsPath(project.id, next))}
          />
        }
        actions={
          board != null && (
            <Button asChild size="sm" variant="secondary">
              <Link to={boardPath(project.id)}>Open board</Link>
            </Button>
          )
        }
      />
      <TabPanel idBase={idBase} value={current}>
        {current === "general" && <ProjectGeneralTab project={project} canEdit={canEdit} />}
        {current === "repositories" && (
          <ProjectRepositoriesTab project={project} canEdit={canEdit} />
        )}
        {current === "members" && <ProjectMembersTab project={project} canEdit={canEdit} />}
        {current === "workflow" && <ProjectWorkflowTab project={project} />}
        {current === "kinds" && <ProjectKindsTab project={project} canEdit={canEdit} />}
        {current === "plugins" && <ProjectPluginsTab project={project} />}
        {current === "import" && <ProjectImportTab project={project} canEdit={canEdit} />}
      </TabPanel>
    </>
  );
}
