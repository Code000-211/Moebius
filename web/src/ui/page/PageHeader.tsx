/**
 * The head of a page. Every page of the app starts with `PageHeader`: one line of 48px with the page's title, then its
 * tabs, and on the right its actions — quiet text buttons and at most one primary. It sticks to the top of the panel
 * over a hairline rule, which is the rule the chosen tab's underline sits on. There is no sentence under the title and
 * no breadcrumb: the sidebar already says where a page lives. A page reached by drilling down can carry the way back
 * on the left of its name, which is where it is looked for.
 *
 * In the desktop app the window buttons sit at the right of the header: the shell reserves their room in
 * `--window-buttons` (nothing by default) and the empty stretch of the header drags the window. `Body` is the sentence
 * inside a box, the same measure and colour, so a page reads the same at either level.
 *
 * ```tsx
 * <PageHeader title="Fleet" tabs={<Tabs … />} actions={<Button size="sm">Pause all</Button>} />
 * ```
 */
import type { ReactNode } from "react";

export function PageHeader({
  title,
  tabs,
  actions,
  /** The way back, for a page that is a thing inside another thing. */
  back,
}: {
  title: string;
  /** The tabs of the page, after its title. A page with nothing to choose between has none. */
  tabs?: ReactNode;
  actions?: ReactNode;
  back?: ReactNode;
  /** Accepted so today's callers compile; the sentence under a title is no longer drawn. */
  context?: ReactNode;
}) {
  return (
    <header
      data-tauri-drag-region
      className="sticky -top-6 z-10 -mx-6 -mt-6 mb-6 flex min-h-12 shrink-0 flex-wrap items-center gap-x-4 gap-y-1 py-1 border-b border-hairline bg-panel pl-6 pr-[calc(1.5rem+var(--window-buttons))]"
    >
      <div className="flex min-w-0 items-center gap-1">
        {back}
        <h1 className="m-0 min-w-0 truncate text-md font-semibold tracking-tight text-text">
          {title}
        </h1>
      </div>
      {tabs && (
        <div className="-mb-1 flex min-w-0 self-stretch [&>[role=tablist]]:items-stretch [&_[role=tab]]:h-full [&_[role=tab]]:pb-1">
          {tabs}
        </div>
      )}
      {actions && (
        <div className="ml-auto flex min-w-0 max-w-full flex-wrap items-center gap-2">
          {actions}
        </div>
      )}
    </header>
  );
}

/** A sentence of a box. */
export function Body({ children }: { children: ReactNode }) {
  return <p className="m-0 max-w-prose text-md text-text">{children}</p>;
}
