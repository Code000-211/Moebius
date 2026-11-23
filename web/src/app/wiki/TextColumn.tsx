/** The middle column of the wiki's three: the text of a page, a new page or the empty state. */
import type { ReactNode } from "react";

export function TextColumn({ children }: { children: ReactNode }) {
  return (
    <div
      data-testid="detail-column"
      className="min-w-0 flex-1 px-8 py-6 lg:min-h-0 lg:overflow-y-auto"
    >
      {children}
    </div>
  );
}
