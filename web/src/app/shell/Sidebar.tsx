/**
 * The sidebar on every page, with the entries of the desktop app's: the app name with the connection, the search
 * (the command bar), Inbox, Chat, Fleet and Automations, then the groups Projects (one entry per board), Build,
 * Improve and Setup. It collapses to a rail of icons. The counts come from the snapshot. The token meter of the
 * desktop sidebar waits for the Usage page.
 */

import {
  CaretLineLeftIcon,
  CaretLineRightIcon,
  type Icon,
  MagnifyingGlassIcon,
  SignOutIcon,
} from "@phosphor-icons/react";
import { useQuery } from "@tanstack/react-query";
import { NavLink } from "react-router";
import { snapshot } from "../../api/calls/board";
import { health } from "../../api/calls/server";
import type { ConnectionState } from "../../api/events";
import { keys } from "../../api/keys";
import { liveRuns } from "../../api/types/board";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "../../ui/components/DropdownMenu";
import { IconButton } from "../../ui/components/IconButton";
import { StatusDot } from "../../ui/components/StatusDot";
import { Tooltip } from "../../ui/components/Tooltip";
import { cn } from "../../ui/lib/cn";
import { getModifierKey } from "../../ui/lib/platform";
import { boardPath, type Group, PAGES } from "../pages";

const STATE: Record<ConnectionState, { label: string; color: string }> = {
  online: { label: "Online", color: "var(--teal)" },
  connecting: { label: "Connecting", color: "var(--text-muted)" },
  reconnecting: { label: "Reconnecting…", color: "var(--amber)" },
};

interface SidebarProps {
  connection: ConnectionState;
  collapsed: boolean;
  onToggle: () => void;
  onSearch: () => void;
  onSignOut: () => void;
}

export function Sidebar({ connection, collapsed, onToggle, onSearch, onSignOut }: SidebarProps) {
  const data = useQuery({ queryKey: keys.snapshot, queryFn: () => snapshot() });
  const info = useQuery({ queryKey: keys.health, queryFn: () => health() });
  const boards = data.data?.boards ?? [];
  const inbox = data.data?.inbox.length ?? 0;
  const working = boards.reduce((sum, board) => sum + liveRuns(board), 0);
  const state = STATE[connection];
  const modifier = getModifierKey();

  const pagesOf = (group: Group) => PAGES.filter((page) => page.group === group);
  const trailingOf = (path: string) => {
    if (path === "inbox" && inbox > 0) return <span className="text-amber">{inbox}</span>;
    if (path === "fleet" && working > 0) return <span className="text-live">{working} live</span>;
    return null;
  };

  return (
    <nav
      aria-label="Pages"
      className={cn(
        "flex h-full shrink-0 flex-col border-r border-sidebar-line bg-sidebar",
        collapsed ? "w-[var(--rail-width)]" : "w-[var(--sidebar-width)]",
      )}
    >
      <div className={cn("flex items-center gap-base p-double", collapsed && "flex-col p-base")}>
        {!collapsed && (
          <div className="min-w-0 flex-1">
            <div className="text-[15px] font-bold text-high">Moebius</div>
            <div
              className="flex items-center gap-half font-mono text-[11px] text-low"
              data-testid="connection"
            >
              <StatusDot color={state.color} />
              {state.label}
            </div>
          </div>
        )}
        {collapsed && <StatusDot color={state.color} />}
        <Tooltip content={collapsed ? "Expand the sidebar" : "Collapse the sidebar"} side="right">
          <IconButton
            icon={collapsed ? CaretLineRightIcon : CaretLineLeftIcon}
            aria-label={collapsed ? "Expand the sidebar" : "Collapse the sidebar"}
            onClick={onToggle}
          />
        </Tooltip>
      </div>

      <div className="px-base pb-base">
        <button
          type="button"
          onClick={onSearch}
          aria-label="Search pages"
          className={cn(
            "flex w-full items-center gap-base rounded-md border border-line bg-recessed px-base py-half text-sm text-low hover:text-high",
            collapsed && "justify-center",
          )}
        >
          <MagnifyingGlassIcon className="size-icon-sm shrink-0" weight="bold" />
          {!collapsed && (
            <>
              <span className="flex-1 text-left">Search</span>
              <kbd className="font-mono text-[11px]">{modifier} K</kbd>
            </>
          )}
        </button>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-base pb-base">
        <Section collapsed={collapsed}>
          {pagesOf("").map((page) => (
            <Entry
              key={page.path}
              to={`/${page.path}`}
              icon={page.icon}
              label={page.title}
              collapsed={collapsed}
              trailing={trailingOf(page.path)}
            />
          ))}
        </Section>
        <Section title="Projects" to="/projects" collapsed={collapsed}>
          {boards.map((board) => {
            const live = liveRuns(board);
            return (
              <Entry
                key={board.repository.id}
                to={boardPath(board.repository.id)}
                dot={live > 0 ? "var(--teal)" : "var(--line)"}
                label={board.repository.name}
                collapsed={collapsed}
                trailing={
                  <span className={live > 0 ? "text-teal" : "text-low"}>
                    {live > 0 ? `${live} live` : "Idle"}
                  </span>
                }
              />
            );
          })}
        </Section>
        {(["Build", "Improve", "Setup"] as const).map((group) => (
          <Section key={group} title={group} collapsed={collapsed}>
            {pagesOf(group).map((page) => (
              <Entry
                key={page.path}
                to={`/${page.path}`}
                icon={page.icon}
                label={page.title}
                collapsed={collapsed}
              />
            ))}
          </Section>
        ))}
      </div>

      <div
        className={cn("border-t border-sidebar-line p-base", collapsed && "flex justify-center")}
      >
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              aria-label="Account"
              className="flex w-full items-center gap-base rounded-md px-base py-half text-sm text-low hover:bg-sidebar-hover hover:text-high"
            >
              <SignOutIcon className="size-icon-sm shrink-0" weight="bold" />
              {!collapsed && (
                <span className="flex-1 text-left">
                  {info.data ? `Server ${info.data.version}` : "Server"}
                </span>
              )}
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent side="top" align="start">
            <DropdownMenuLabel>
              {info.data ? `Moebius ${info.data.version}` : "Moebius"}
            </DropdownMenuLabel>
            <DropdownMenuSeparator />
            <DropdownMenuItem onSelect={onSignOut}>Sign out</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </nav>
  );
}

function Section({
  title,
  to,
  collapsed,
  children,
}: {
  title?: string;
  to?: string;
  collapsed: boolean;
  children: React.ReactNode;
}) {
  return (
    <div className="border-b border-sidebar-line py-base last:border-b-0">
      {title && !collapsed && (
        <div className="px-base pb-half font-mono text-[11px] uppercase tracking-wide text-sidebar-group">
          {to ? (
            <NavLink to={to} className="hover:text-high">
              {title}
            </NavLink>
          ) : (
            title
          )}
        </div>
      )}
      <ul className="flex flex-col gap-[2px]">{children}</ul>
    </div>
  );
}

function Entry({
  to,
  icon: IconComponent,
  dot,
  label,
  collapsed,
  trailing,
}: {
  to: string;
  icon?: Icon;
  dot?: string;
  label: string;
  collapsed: boolean;
  trailing?: React.ReactNode;
}) {
  const link = (
    <NavLink
      to={to}
      aria-label={collapsed ? label : undefined}
      className={({ isActive }) =>
        cn(
          "flex items-center gap-base rounded-md px-base py-half text-sm",
          isActive
            ? "bg-sidebar-current text-accent"
            : "text-sidebar-text hover:bg-sidebar-hover hover:text-high",
          collapsed && "justify-center",
        )
      }
    >
      {IconComponent && <IconComponent className="size-icon-sm shrink-0" weight="regular" />}
      {dot && <StatusDot color={dot} />}
      {!collapsed && (
        <>
          <span className="min-w-0 flex-1 truncate">{label}</span>
          {trailing && <span className="font-mono text-[11px]">{trailing}</span>}
        </>
      )}
    </NavLink>
  );
  return (
    <li>
      {collapsed ? (
        <Tooltip content={label} side="right">
          {link}
        </Tooltip>
      ) : (
        link
      )}
    </li>
  );
}
