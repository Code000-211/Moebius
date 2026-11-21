/**
 * The header every page of a project shares: the project's name, its tabs (Board, Workflows, Cycles, Milestones, and
 * Intake with the number of requests waiting only while the project takes requests), and at the right a gear to the
 * project's settings and a `...` menu with Import from Jira (an owner's) and Terminal. The tab is in the address
 * (`/projects/:id/<tab>`), so a link and a reload keep it. The board passes its own controls as `actions`, and the
 * layout switch as `layout`, which only the Board tab has.
 */
import { DotsThreeIcon, GearIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { type ReactNode, useId } from "react";
import { Link, useNavigate } from "react-router";
import { listRequests } from "../../api/calls/intake";
import { keys } from "../../api/keys";
import type { Project } from "../../api/types/projects";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "../../ui/components/DropdownMenu";
import { PageHeader } from "../../ui/page/PageHeader";
import { Tabs } from "../../ui/page/Tabs";
import { type ProjectTab, projectTabPath, settingsPath } from "../pages";
import { useRole } from "../shell/session";

const LABELS: Record<ProjectTab, string> = {
  workflows: "Workflows",
  cycles: "Cycles",
  milestones: "Milestones",
  intake: "Intake",
};

/** The quiet square the gear and the menu share. */
const SQUARE =
  "inline-flex size-8 shrink-0 cursor-pointer items-center justify-center rounded-control bg-transparent text-secondary transition-colors duration-(--motion-routine) ease-out hover:bg-hover hover:text-text data-[state=open]:bg-hover data-[state=open]:text-text";

export function ProjectHeader({
  project,
  tab,
  layout,
  actions,
  primary,
  onTerminal,
}: {
  project: Project;
  tab: "board" | ProjectTab;
  /** The Board/List switch, drawn after the tabs; only the board has one. */
  layout?: ReactNode;
  /** The page's own controls, before the gear. */
  actions?: ReactNode;
  /** The one primary button of the page, at the far right. */
  primary?: ReactNode;
  /** Opens the shell of the project's repository. */
  onTerminal: () => void;
}) {
  const base = useId();
  const navigate = useNavigate();
  const role = useRole();
  // Only a project that takes requests has the tab, and then the count of those a member has yet to decide.
  const requests = useQuery({
    queryKey: keys.requests(project.id),
    queryFn: () => listRequests(project.id),
    enabled: project.intake,
  });
  const waiting = requests.data?.filter((each) => each.status === "drafted").length ?? 0;
  const tabs: Array<{ value: "board" | ProjectTab; label: string; count?: number }> = [
    { value: "board", label: "Board" },
    { value: "workflows", label: LABELS.workflows },
    { value: "cycles", label: LABELS.cycles },
    { value: "milestones", label: LABELS.milestones },
    ...(project.intake ? [{ value: "intake" as const, label: LABELS.intake, count: waiting }] : []),
  ];
  return (
    <PageHeader
      title={project.name}
      tabs={
        <Tabs
          tabs={tabs}
          value={tab}
          onChange={(next) => navigate(projectTabPath(project.id, next))}
          label="Project"
          idBase={base}
        />
      }
      actions={
        <>
          {layout}
          {actions}
          <Link
            to={settingsPath(project.id)}
            aria-label="Project settings"
            title="Project settings"
            className={SQUARE}
          >
            <GearIcon className="size-4" weight="bold" aria-hidden="true" />
          </Link>
          <DropdownMenu>
            <DropdownMenuTrigger aria-label="More project actions" className={SQUARE}>
              <DotsThreeIcon className="size-5" weight="bold" aria-hidden="true" />
            </DropdownMenuTrigger>
            <DropdownMenuContent align="end">
              {role === "owner" && (
                <DropdownMenuItem onSelect={() => navigate(`${settingsPath(project.id)}#import`)}>
                  Import from Jira
                </DropdownMenuItem>
              )}
              <DropdownMenuItem onSelect={onTerminal}>Terminal</DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
          {primary}
        </>
      }
    />
  );
}
