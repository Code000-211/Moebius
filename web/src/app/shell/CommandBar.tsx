/**
 * The command bar, opened with Ctrl+K (Cmd+K on a Mac) or the sidebar's search: it lists the pages and the boards in
 * the groups of the sidebar and opens the one chosen. Built from the Command primitives copied from Vibe Kanban
 * (`ui/components/Command.tsx`); its `CommandBar` carries issues, branches and priorities that Moebius does not have.
 */

import { FolderIcon } from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "react-router";
import { snapshot } from "../../api/calls/board";
import { keys } from "../../api/keys";
import {
  Command,
  CommandDialog,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "../../ui/components/Command";
import { boardPath, type Group, PAGES } from "../pages";

const GROUPS: readonly Group[] = ["", "Projects", "Build", "Improve", "Setup"];

export function CommandBar({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const navigate = useNavigate();
  const data = useQuery({ queryKey: keys.snapshot, queryFn: () => snapshot(), enabled: open });

  function go(path: string) {
    onOpenChange(false);
    void navigate(path);
  }

  return (
    <CommandDialog open={open} onOpenChange={onOpenChange}>
      <Command className="[&_[cmdk-group-heading]]:px-base">
        <CommandInput placeholder="Go to a page…" />
        <CommandList>
          <CommandEmpty>No page matches.</CommandEmpty>
          {GROUPS.map((group) => {
            const pages = PAGES.filter((page) => page.group === group);
            const boards = group === "Projects" ? (data.data?.boards ?? []) : [];
            return (
              <CommandGroup key={group || "pages"} heading={group || "Pages"}>
                {pages.map((page) => (
                  <CommandItem
                    key={page.path}
                    value={page.title}
                    onSelect={() => go(`/${page.path}`)}
                  >
                    <page.icon className="size-icon-sm" weight="regular" />
                    <span>{page.title}</span>
                  </CommandItem>
                ))}
                {boards.map((board) => (
                  <CommandItem
                    key={board.repository.id}
                    value={`Board ${board.repository.name}`}
                    onSelect={() => go(boardPath(board.repository.id))}
                  >
                    <FolderIcon className="size-icon-sm" weight="regular" />
                    <span>{board.repository.name}</span>
                  </CommandItem>
                ))}
              </CommandGroup>
            );
          })}
        </CommandList>
      </Command>
    </CommandDialog>
  );
}
