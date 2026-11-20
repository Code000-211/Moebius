/**
 * The state of a place with nothing in it yet: an icon, one short line and one action. It replaces a blank panel and
 * carries an action only when there is one to take. It is never a card: it is drawn straight on the panel, with no
 * surface, outline or shadow of its own, so it reads the same on a page, in a lane of the board and inside a box.
 *
 * ```tsx
 * <Empty icon={<Check />} title="Nothing needs you" action={<Button>Open Fleet</Button>} />
 * ```
 */
import type { ReactNode } from "react";
import { cn } from "../lib/cn";

export function Empty({
  title,
  body,
  icon,
  action,
  centered = false,
  className,
}: {
  /** The one short line: "Nothing needs you". */
  title?: string;
  /** How to begin, when the line alone does not say. */
  body?: ReactNode;
  /** The shape of the state, above its line. It leans on the line, so it is hidden from a screen reader. */
  icon?: ReactNode;
  /** The first thing to do, when there is one. */
  action?: ReactNode;
  /** Kept so today's callers compile: an empty state is never a card now, so there is nothing to turn off. */
  plain?: boolean;
  /** The words in the middle of the place, which the state takes the whole of, as a column with no cards in it. */
  centered?: boolean;
  className?: string;
}) {
  return (
    <section
      className={cn(
        "flex flex-col gap-3 py-6",
        centered ? "flex-1 items-center justify-center text-center" : "items-start",
        className,
      )}
    >
      {icon && (
        <span aria-hidden="true" className="flex shrink-0 text-text-faint">
          {icon}
        </span>
      )}
      {title && <h2 className="m-0 text-base font-medium text-text">{title}</h2>}
      {body && <p className="m-0 max-w-prose text-base text-muted">{body}</p>}
      {action}
    </section>
  );
}
