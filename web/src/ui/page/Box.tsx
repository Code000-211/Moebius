/**
 * A box of a page: a section set apart by a hairline above it, with a quiet heading, a line that sums it up at its
 * right, the box's own actions, and the content. It has no shadow, no outline and no fill, so it is never a card, and
 * nothing in it is a card either. The Usage, Settings, Projects, Agents and Models pages are made of these.
 *
 * ```tsx
 * <Box title="Repositories" actions={<Button size="sm">Add</Button>}>…</Box>
 * ```
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
      className={cn("border-t border-hairline", padded && "py-4", className)}
    >
      {(title || aside || actions) && (
        <header className="mb-3 flex flex-wrap items-baseline justify-between gap-3">
          {title && (
            <h2 id={headingId} className="m-0 text-base font-medium text-secondary">
              {title}
            </h2>
          )}
          {aside && <span className="text-sm text-muted">{aside}</span>}
          {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
        </header>
      )}
      {children}
    </section>
  );
}
