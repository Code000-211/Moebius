/**
 * The head of a page and the sentence that follows it. Every page of the app starts with `PageHeader`: what it is
 * called, one short sentence of what belongs here, and the page's own actions on the right. There is no breadcrumb —
 * the sidebar already says which group a page sits in, and a page that repeats it spends a line to say nothing new.
 * A page reached by drilling down can carry the way back on the left of its name, which is where it is looked for and
 * which says one thing rather than spelling out a path. `Body` is the sentence inside a box, the same measure and
 * colour, so a page reads the same at either level.
 */
import type { ReactNode } from "react";

/** The title of a page with the sentence under it and the page's actions on the right. */
export function PageHeader({
  title,
  context,
  actions,
  /** The way back, for a page that is a thing inside another thing. */
  back,
}: {
  title: string;
  context?: ReactNode;
  actions?: ReactNode;
  back?: ReactNode;
}) {
  return (
    <header className="mb-6 flex flex-wrap items-start justify-between gap-4">
      <div className="min-w-0">
        <div className="flex min-w-0 items-center gap-1">
          {back}
          <h1 className="m-0 min-w-0 truncate text-2xl font-semibold tracking-tight text-text">
            {title}
          </h1>
        </div>
        {context && <p className="mt-2 max-w-prose text-base text-secondary">{context}</p>}
      </div>
      {actions && <div className="flex items-center gap-2">{actions}</div>}
    </header>
  );
}

/** A sentence of a box. */
export function Body({ children }: { children: ReactNode }) {
  return <p className="m-0 max-w-prose text-base leading-relaxed text-text">{children}</p>;
}
