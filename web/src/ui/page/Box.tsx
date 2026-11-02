/**
 * A box of a page: a heading, a line that sums it up at its right, the box's own actions, and the content. The Usage,
 * Settings, Projects, Agents and Models pages are made of these. It is a card, so it is borderless and rounded at 16
 * and marks its layer with a soft shadow rather than a line.
 */
import { type ReactNode, useId } from "react";
import { cn } from "../lib/cn";

export function Box({
  title,
  aside,
  actions,
  children,
  className,
  testId,
  padded = true,
}: {
  /** What the box holds, said plainly. A link or a count may sit inside it, so it is not only text. */
  title?: ReactNode;
  aside?: ReactNode;
  /** What a person can do with the box, at the right of its heading. */
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
  /** For the tests that watch one box on a page full of them. */
  testId?: string;
  /** Off when the content brings its own rows edge to edge, as a list does. */
  padded?: boolean;
}) {
  const headingId = useId();
  return (
    <section
      aria-labelledby={title ? headingId : undefined}
      data-testid={testId}
      className={cn(
        "rounded-card bg-surface shadow-card",
        padded && "p-4",
        "transition-shadow duration-(--motion-routine)",
        className,
      )}
    >
      {(title || aside || actions) && (
        <header className="mb-3 flex flex-wrap items-baseline justify-between gap-3">
          {title && (
            <h2 id={headingId} className="m-0 text-lg font-semibold tracking-tight text-text">
              {title}
            </h2>
          )}
          {aside && <span className="text-sm text-secondary">{aside}</span>}
          {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
        </header>
      )}
      {children}
    </section>
  );
}
