/**
 * The two containers of the sidebar's list: a group with its label (`Section`) and Improve as one row whose pages open
 * under it (`ImproveRow`). `Sidebar` and the sections that stand in it build their lists out of these.
 */

import { ArrowsClockwiseIcon, CaretDownIcon } from "@phosphor-icons/react";
import { cn } from "../../ui/lib/cn";

/**
 * Improve as one row whose pages open under it. The caret beside its name is the toggle, and the state is kept for
 * this browser. The rail has no room for the row, so it draws the pages alone, every one of them standing.
 */
export function ImproveRow({
  collapsed,
  open,
  onToggle,
  children,
}: {
  collapsed: boolean;
  open: boolean;
  onToggle: () => void;
  children: React.ReactNode;
}) {
  if (collapsed) return <>{children}</>;
  return (
    <li>
      <button
        type="button"
        onClick={onToggle}
        aria-expanded={open}
        aria-label={`${open ? "Collapse" : "Expand"} Improve`}
        className={cn(
          "flex min-h-8 w-full cursor-pointer items-center gap-2 rounded-control px-3 text-base text-sidebar-text",
          "transition-colors duration-(--motion-routine) ease-out hover:bg-hover hover:text-text",
        )}
      >
        <ArrowsClockwiseIcon className="size-5 shrink-0" weight="regular" />
        <span className="min-w-0 flex-1 truncate text-left">Improve</span>
        <CaretDownIcon
          className={cn(
            "size-3 shrink-0 transition-transform duration-(--motion-routine)",
            !open && "-rotate-90",
          )}
          weight="bold"
        />
      </button>
      {open && (
        <>
          {/* biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list
              with no marker from the accessibility tree entirely */}
          <ul role="list" className="mt-1 flex flex-col gap-1 pl-4">
            {children}
          </ul>
        </>
      )}
    </li>
  );
}

/**
 * One group of the sidebar's list. Its label and the space around it say where it starts and ends; there is no
 * rule under it, because the rail has no label to draw and a line there would divide nothing. The label names the
 * group and goes nowhere — a way in is a row, at the height of the rows beside it.
 */
export function Section({
  title,
  collapsed,
  children,
}: {
  title?: string;
  collapsed: boolean;
  children: React.ReactNode;
}) {
  return (
    <div className="py-2">
      {title &&
        (collapsed ? (
          // The rail has no room for the group's name, but it keeps the room its line takes, so the rows under it do
          // not climb when the sidebar narrows. The rail never folds a group: every row stays where it always was.
          <div className="mb-2 h-4 px-2" aria-hidden="true" />
        ) : (
          <div className="mb-2 flex h-4 items-center px-2 text-sm leading-4 font-semibold tracking-wide text-sidebar-group [font-variant-caps:all-small-caps]">
            {title}
          </div>
        ))}
      {/* biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with
          no marker from the accessibility tree entirely */}
      <ul role="list" className="flex flex-col gap-1">
        {children}
      </ul>
    </div>
  );
}
