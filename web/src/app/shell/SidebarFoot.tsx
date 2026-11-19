/**
 * The rows of the sidebar that belong to the session rather than to a page: the way back out of the settings list, whom
 * the browser is signed in as, the way out of the session, and the gear to Settings at the foot of every view.
 */

import { ArrowLeftIcon, GearIcon, SignOutIcon } from "@phosphor-icons/react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link } from "react-router";
import { getSession } from "../../api/calls/session";
import { keys } from "../../api/keys";
import { Tooltip } from "../../ui/components/Tooltip";
import { cn } from "../../ui/lib/cn";
import { leaveSession } from "./session";

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
 * Whom this browser is signed in as, above the way out: a line of text and no row to press, since it goes nowhere. It
 * reads the session the app already holds and never asks for it itself.
 */
export function SignedInAs() {
  const session = useQuery({ queryKey: keys.session, queryFn: getSession, enabled: false });
  const person = session.data?.person;
  if (!person) return null;
  return (
    <li className="flex min-h-8 min-w-0 items-center px-3 text-sm text-secondary">
      <span className="truncate">
        Signed in as {person.name} <span className="text-text">@{person.handle}</span>
      </span>
    </li>
  );
}

/**
 * The way out of the session. It is an item of the Settings list rather than a page of its own: it leaves nothing
 * behind to look at, so there is nothing a page could say that the row does not.
 */
export function SignOut({ collapsed }: { collapsed: boolean }) {
  const queryClient = useQueryClient();
  const [leaving, setLeaving] = useState(false);
  const name = leaving ? "Signing out" : "Sign out";
  const row = (
    <button
      type="button"
      aria-label={collapsed ? name : undefined}
      disabled={leaving}
      className={cn(
        "flex min-h-8 w-full cursor-pointer items-center gap-2 rounded-control px-3 text-base text-sidebar-text",
        "transition-colors duration-(--motion-routine) ease-out hover:bg-hover hover:text-text",
        "disabled:cursor-default disabled:opacity-45",
        collapsed && "justify-center px-0",
      )}
      onClick={() => {
        setLeaving(true);
        void leaveSession(queryClient);
      }}
    >
      <SignOutIcon className="size-5 shrink-0" weight="regular" />
      {!collapsed && <span className="min-w-0 flex-1 truncate text-left">{name}</span>}
    </button>
  );
  // The row's own words are enough while they are on screen; only the rail has to say them again.
  return (
    <li>
      {collapsed ? (
        <Tooltip content={name} side="right">
          {row}
        </Tooltip>
      ) : (
        row
      )}
    </li>
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
