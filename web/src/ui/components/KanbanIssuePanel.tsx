// From BloopAI/vibe-kanban packages/ui/src/components/KanbanIssuePanel.tsx (Apache-2.0), changed: only the section
// of the panel is left. The panel itself (its header, Escape handling and scrolling body) is the card window of
// `app/board/CardWindow.tsx`, a centred dialog over the page, so the drawer's frame and the scroll mask that faded the
// edges of its body are gone. A section is a heading and its content, set apart from the next by space and not by a rule.
import { type ReactNode, useId } from "react";
import { cn } from "../lib/cn";

/** A section of the card window: a heading and its content, set apart by space rather than by a rule. */
export function KanbanIssueSection({
  title,
  children,
  className,
}: {
  title: string;
  children: ReactNode;
  className?: string;
}) {
  // The heading says the title once; the section is named by pointing at it rather than by repeating the words.
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className={cn("px-6 pt-4 pb-4", className)}>
      <h3 id={headingId} className="m-0 mb-3 text-sm font-semibold text-secondary">
        {title}
      </h3>
      {children}
    </section>
  );
}
