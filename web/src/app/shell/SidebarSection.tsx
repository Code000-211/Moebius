/**
 * One group of the sidebar's list: its small muted name, which is a link when the group's name is a place of its
 * own (Projects), and the rows under it. A group row is 30px, an item row 36px; there is no rule between groups, only
 * the air around them.
 */

import { Link } from "react-router";
import { cn } from "../../ui/lib/cn";

/** The classes of a group's name. */
const GROUP = "flex h-[30px] items-center rounded-control px-2.5 text-sm text-sidebar-group";

export function Section({
  title,
  to,
  children,
}: {
  title?: string;
  /** Where the name leads, when the group is also a place. */
  to?: string;
  children?: React.ReactNode;
}) {
  return (
    <div className="pt-4 first:pt-0">
      {title &&
        (to ? (
          <Link to={to} className={cn(GROUP, "hover:bg-row-hover hover:text-text")}>
            {title}
          </Link>
        ) : (
          <div className={GROUP}>{title}</div>
        ))}
      {/* biome-ignore lint/a11y/noRedundantRoles: preflight sets list-style: none, and WebKit then drops a list with
          no marker from the accessibility tree entirely */}
      <ul role="list" className="flex flex-col">
        {children}
      </ul>
    </div>
  );
}
