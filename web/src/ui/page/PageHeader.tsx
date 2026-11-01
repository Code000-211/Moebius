/**
 * The head of a page and the sentence that follows it. Every page of the app starts with `PageHeader`: what it is
 * called, one short sentence of what belongs here, and the page's own actions on the right. There is no breadcrumb —
 * the sidebar already says which group a page sits in, and a page that repeats it spends a line to say nothing new.
 * `Body` is the sentence inside a box, the same measure and colour, so a page reads the same at either level.
 */
import type { ReactNode } from "react";

/** The title of a page with the sentence under it and the page's actions on the right. */
export function PageHeader({
  title,
  context,
  actions,
}: {
  title: string;
  context?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <header className="mb-6 flex flex-wrap items-start justify-between gap-4">
      <div className="min-w-0">
        <h1 className="m-0 text-2xl font-semibold tracking-tight text-text">{title}</h1>
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
