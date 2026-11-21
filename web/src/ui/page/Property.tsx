/**
 * One property of a thing, as a row of the page's content or of a window: its name, and its value beside it. A group
 * of them sits in a `<section aria-label>` of the page (the facts of a Retro's day, a workflow's definition, the card
 * window's properties); there is no column of its own for them.
 */
import type { ReactNode } from "react";

export function Property({ name, children }: { name: string; children: ReactNode }) {
  return (
    <div className="grid min-h-8 grid-cols-[6rem_minmax(0,1fr)] items-center gap-2 text-sm">
      <span className="text-secondary">{name}</span>
      <span className="min-w-0 break-words text-text">{children}</span>
    </div>
  );
}
