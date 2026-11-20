/**
 * The rows of the sidebar that belong to the session rather than to a page: the way back out of the settings list and the gear to Settings at the foot of every view.
 */

import { ArrowLeftIcon, GearIcon } from "@phosphor-icons/react";
import { Link } from "react-router";
import { Tooltip } from "../../ui/components/Tooltip";
import { cn } from "../../ui/lib/cn";

/**
 * The way out of the settings list, and the only row above the list itself. It is a row rather than the mark at the
 * top of the sidebar because it is the same kind of thing as the rows under it: it goes somewhere, and it is found by
 * the same scan of the list that finds everything else.
 */
export function Back({ collapsed, to }: { collapsed: boolean; to: string }) {
  const name = "Back to the pages";
  const link = (
    <Link
      to={to}
      aria-label={collapsed ? name : undefined}
      className={cn(
        "flex min-h-8 items-center gap-2 rounded-control px-3 text-base text-sidebar-text",
        "transition-colors duration-(--motion-routine) ease-out hover:bg-hover hover:text-text",
        collapsed && "justify-center px-0",
      )}
    >
      <ArrowLeftIcon className="size-5 shrink-0" weight="regular" />
      {!collapsed && <span className="min-w-0 flex-1 truncate">Back</span>}
    </Link>
  );
  return collapsed ? (
    <Tooltip content={name} side="right">
      {link}
    </Tooltip>
  ) : (
    link
  );
}

/**
 * The way to Settings at the foot of the list, on every view of it: the everyday list's, the settings list's and the
 * rail's, because the foot is the one row the list never exchanges — a hand that learned its place keeps it while the
 * pages above it are swapped. It is drawn as the rail draws a row, the gear alone at the height of every row, so it
 * does not change shape when the sidebar does; the words a wide row would carry beside it are the tooltip's and the
 * link's name, and the list above the foot says which page is on screen, so the foot says nothing about current.
 */
export function SettingsFoot() {
  const name = "Settings";
  const link = (
    <Link
      to="/settings"
      aria-label={name}
      className={cn(
        "flex min-h-8 items-center gap-2 rounded-control px-3 text-base",
        "justify-center px-0",
        "text-sidebar-text hover:bg-hover hover:text-text",
      )}
    >
      <GearIcon className="size-5 shrink-0" weight="regular" />
    </Link>
  );
  return (
    <li>
      <Tooltip content={name} side="right">
        {link}
      </Tooltip>
    </li>
  );
}
