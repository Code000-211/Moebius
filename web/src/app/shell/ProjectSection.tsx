/**
 * The project switcher and the places of the chosen project under it, as the sidebar's list shows them: the raised row
 * that opens the list of projects, the project's Board with its live work, and its Files.
 */

import { CaretDownIcon, FilesIcon, FolderIcon, SquaresFourIcon } from "@phosphor-icons/react";
import { Link } from "react-router";
import type { Snapshot } from "../../api/types/board";
import { liveRuns } from "../../api/types/board";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "../../ui/components/DropdownMenu";
import { Tooltip } from "../../ui/components/Tooltip";
import { cn } from "../../ui/lib/cn";
import { boardPath } from "../pages";
import { Entry } from "./SidebarEntry";
import { Section } from "./SidebarSection";

/**
 * The project switcher and the places of the chosen project under it: its Board and its Files. The switcher is a
 * raised row that opens the list of the projects of the snapshot, with All projects at the end of it; choosing one
 * leads to its board. In the rail it is the folder alone, its words in its name and its tooltip.
 */
export function ProjectSection({
  boards,
  chosen,
  collapsed,
  at,
  here,
  onChoose,
}: {
  boards: Snapshot["boards"];
  chosen: number | null;
  collapsed: boolean;
  at: string;
  here: string;
  onChoose: (id: number) => void;
}) {
  const board = boards.find((each) => each.repository.id === chosen);
  const live = board ? liveRuns(board) : 0;
  const name = board ? `Project ${board.repository.name}` : "Projects";
  const trigger = (
    <button
      type="button"
      aria-label={collapsed ? `${name}. Change project` : "Change project"}
      title={collapsed ? undefined : board?.repository.name}
      className={cn(
        "flex min-h-8 w-full cursor-pointer items-center gap-2 rounded-control bg-raised px-3 text-base text-text shadow-card",
        "transition-colors duration-(--motion-routine) ease-out hover:bg-hover",
        collapsed && "justify-center px-0",
      )}
    >
      <FolderIcon className="size-5 shrink-0" weight="regular" />
      {!collapsed && (
        <>
          <span className="min-w-0 flex-1 truncate text-left">
            {board?.repository.name ?? "Projects"}
          </span>
          <CaretDownIcon className="size-4 shrink-0 text-secondary" weight="bold" />
        </>
      )}
    </button>
  );
  const standsAt = chosen === null ? "" : boardPath(chosen);
  return (
    <Section collapsed={collapsed}>
      <li>
        <DropdownMenu modal={false}>
          {collapsed ? (
            <Tooltip content={name} side="right">
              <DropdownMenuTrigger asChild>{trigger}</DropdownMenuTrigger>
            </Tooltip>
          ) : (
            <DropdownMenuTrigger asChild>{trigger}</DropdownMenuTrigger>
          )}
          <DropdownMenuContent align="start">
            <DropdownMenuRadioGroup
              value={chosen === null ? "" : String(chosen)}
              onValueChange={(next) => onChoose(Number(next))}
            >
              {boards.map((each) => (
                <DropdownMenuRadioItem key={each.repository.id} value={String(each.repository.id)}>
                  {each.repository.name}
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
            <DropdownMenuItem asChild>
              <Link to="/projects">All projects</Link>
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </li>
      {chosen !== null && (
        <>
          <Entry
            to={standsAt}
            icon={SquaresFourIcon}
            label="Board"
            collapsed={collapsed}
            // The script of a column is a page of the same project, so it keeps its row marked.
            current={at === standsAt || at.startsWith(`${standsAt}/`)}
            mark={{
              color: live > 0 ? "var(--progress)" : "var(--hairline-strong)",
              word: live > 0 ? `${live}` : "Idle",
              tone: "progress",
              printed: live > 0,
            }}
          />
          <Entry
            to={`/projects/${chosen}`}
            icon={FilesIcon}
            label="Files"
            collapsed={collapsed}
            current={at === `/projects/${chosen}`}
          />
        </>
      )}
      {/* The list of every project is a page of its own, reached from the switcher and from here when no project is
          chosen yet. */}
      {chosen === null && (
        <Entry
          to="/projects"
          icon={SquaresFourIcon}
          label="All projects"
          collapsed={collapsed}
          current={here === "projects"}
        />
      )}
    </Section>
  );
}
