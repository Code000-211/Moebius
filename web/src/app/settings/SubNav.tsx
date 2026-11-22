/**
 * The row of a sub-navigation, in Settings and in a project's settings (`design/navigation/ia3.png`, `ia4.png`): an
 * icon and the name on a row of 28px, the chosen one a grey pill (`--nav-selected`, radius 8) in medium weight. The
 * groups are named in muted text above their rows; there is no search field.
 */
import type { Icon } from "@phosphor-icons/react";
import { cn } from "../../ui/lib/cn";

/** The classes of a row, chosen or not. */
export function subNavRow(active: boolean): string {
  return cn(
    "flex h-7 items-center gap-2.5 rounded-card px-2 text-base",
    active ? "bg-nav-selected font-medium text-text" : "text-secondary hover:bg-hover",
  );
}

/** The icon and the name of a row. */
export function SubNavLabel({ icon: Glyph, label }: { icon: Icon; label: string }) {
  return (
    <>
      <Glyph className="size-4 shrink-0 text-muted" aria-hidden="true" />
      <span className="min-w-0 truncate">{label}</span>
    </>
  );
}

/** The name of a group, above its rows. */
export const SUB_NAV_GROUP = "px-2 pb-1 text-sm leading-5 text-muted";
