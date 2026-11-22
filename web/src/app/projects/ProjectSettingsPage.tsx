/**
 * A project's settings in four areas, in a sub-navigation at the left as Settings has it (ia3.png; `/projects/:projectId/settings/:tab`, from the gear of the project's header):
 * General (its name, its key, private, intake and the kinds of issues it offers), Members, Repositories (with their
 * home project; a row opens the repository's page, which holds the settings of its code and its plugins) and Columns
 * (the script of each column, at `settings/columns/:column`). Every setting of a project is edited and saved in its
 * tab. Only an owner edits: anyone else who sees the project reads it. The tab is part of the address, so a tab can be
 * linked to; the tabs that are gone (Workflow, Kinds, Plugins, Import) are in the legacy table.
 */
import { ArrowLeftIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { Link, useParams, useSearchParams } from "react-router";
import { listProjects } from "../../api/calls/projects";
import { keys } from "../../api/keys";
import { Button } from "../../ui/components/Button";
import { Tooltip } from "../../ui/components/Tooltip";
import { cn } from "../../ui/lib/cn";
import { Empty } from "../../ui/page/Empty";
import { InSettingsArea, PageHeader } from "../../ui/page/PageHeader";
import { ReadFailed } from "../../ui/page/ReadFailed";
import { SkeletonCard } from "../../ui/page/Skeleton";
import { boardPath, settingsPath } from "../pages";
import { useRole } from "../shell/session";
import { ProjectColumnsTab } from "./ProjectColumnsTab";
import { ProjectGeneralTab } from "./ProjectGeneralTab";
import { ProjectMembersTab } from "./ProjectMembersTab";
import { ProjectRepositoriesTab } from "./ProjectRepositoriesTab";

const TABS = ["general", "members", "repositories", "columns"] as const;
type Tab = (typeof TABS)[number];

const LABELS: Record<Tab, string> = {
  general: "General",
  members: "Members",
  repositories: "Repositories",
  columns: "Columns",
};

export function ProjectSettingsPage() {
  const { projectId: id, tab, column } = useParams();
  const [search] = useSearchParams();
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
  // An address with a column (`settings/columns/:column`) is the Columns tab.
  const current: Tab =
    TABS.find((each) => each === (column === undefined ? tab : "columns")) ?? "general";
  const canEdit = role === undefined || role === "owner";
  const repository = Number(search.get("repository")) || undefined;
  return (
    <div className="-m-4 flex h-[calc(100%+2rem)] flex-col">
      <div className="shrink-0 px-4 pt-4 [&>header]:mb-0 [&>header]:static">
        <PageHeader
          title={`${project.name} › Settings`}
          back={
            <Tooltip content="Back to the board" side="bottom">
              <Button asChild variant="icon" aria-label="Back to the board">
                <Link to={boardPath(project.id, repository)}>
                  <ArrowLeftIcon className="size-5" />
                </Link>
              </Button>
            </Tooltip>
          }
        />
      </div>
      <div className="flex min-h-0 flex-1 items-stretch">
        <nav
          aria-label="Project settings"
          className="flex w-50 shrink-0 flex-col gap-0.5 overflow-y-auto border-r border-hairline px-2 py-2"
        >
          <span className="truncate px-2 pb-1 text-sm leading-5 text-secondary">
            {project.name}
          </span>
          {TABS.map((each) => (
            <Link
              key={each}
              to={settingsPath(project.id, each)}
              aria-current={each === current ? "page" : undefined}
              className={cn(
                "flex h-7 items-center rounded-control px-2 text-base",
                each === current
                  ? "bg-selected font-medium text-accent-text"
                  : "text-text hover:bg-hover",
              )}
            >
              {LABELS[each]}
            </Link>
          ))}
        </nav>
        <div className="min-w-0 flex-1 overflow-y-auto px-10 pb-6 pt-5">
          <InSettingsArea.Provider value>
            <PageHeader title={LABELS[current]} />
          </InSettingsArea.Provider>
          {current === "general" && <ProjectGeneralTab project={project} canEdit={canEdit} />}
          {current === "members" && <ProjectMembersTab project={project} canEdit={canEdit} />}
          {current === "repositories" && (
            <ProjectRepositoriesTab project={project} canEdit={canEdit} />
          )}
          {current === "columns" && (
            <ProjectColumnsTab project={project} column={column} canEdit={canEdit} />
          )}
        </div>
      </div>
    </div>
  );
}
