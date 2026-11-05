/** One labelled fact of the dock: a name on the left and what it says, which wraps, on the right. */
import type { ReactNode } from "react";

export function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex flex-wrap items-baseline gap-x-4 gap-y-1">
      <dt className="w-20 shrink-0 text-sm font-semibold text-secondary">{label}</dt>
      <dd className="m-0 min-w-0 flex-1 text-base text-text [overflow-wrap:anywhere]">
        {children}
      </dd>
    </div>
  );
}
