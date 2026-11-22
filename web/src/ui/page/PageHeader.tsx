/**
 * The head of a page. Every page of the app starts with `PageHeader`: one line of 48px with the page's title, then its
 * tabs, and on the right its actions — quiet text buttons and at most one primary. It sticks to the top of the panel
 * over a hairline rule, which is the rule the chosen tab's underline sits on. There is no sentence under the title and
 * no breadcrumb: the sidebar already says where a page lives. A page reached by drilling down can carry the way back
 * on the left of its name, which is where it is looked for.
 *
 * At the right, in this order on every page: the page's own `actions` (quiet), then its `menu` (a gear or a `...`), then
 * the "Moebius" button that opens the agent's panel (`app/shell/AgentButton.tsx`), and last the page's one `primary`
 * button. So the button that does the page's main thing is always the rightmost and Moebius never moves between pages.
 *
 * In a narrow window (under 1000px) the sidebar is hidden and a button at the left of the header brings it back over
 * the page.
 *
 * In the desktop app the window buttons sit at the right of the header: the shell reserves their room in
 * `--window-buttons` (nothing by default) and the empty stretch of the header drags the window. `Body` is the sentence
 * inside a box, the same measure and colour, so a page reads the same at either level.
 *
 * ```tsx
 * <PageHeader title="Fleet" tabs={<Tabs … />} actions={<Button size="sm">Pause all</Button>} />
 * ```
 */
import { createContext, type ReactNode, useContext } from "react";
import { AgentButton } from "../../app/shell/AgentButton";
import { useSidebarToggle } from "../../app/shell/SidebarToggle";
import { IconButton } from "../components/IconButton";
import { SidebarIcon } from "../icons";
import { cn } from "../lib/cn";

/**
 * True inside the content of a Settings area. There a page's title is the area's title, the one every area has: in
 * the same place and the same size as Appearance's, without the sticky bar across the panel, so the title, its rows
 * and their edges share one column.
 */
export const InSettingsArea = createContext(false);

export function PageHeader({
  title,
  tabs,
  actions,
  menu,
  primary,
  /** The way back, for a page that is a thing inside another thing. */
  back,
  /** Keeps the header to one row of 48px however narrow it gets: the tabs, not the buttons, give way. */
  oneRow = false,
}: {
  title: string;
  /** The tabs of the page, after its title. A page with nothing to choose between has none. */
  tabs?: ReactNode;
  /** The page's own quiet controls. */
  actions?: ReactNode;
  /** The gear or the `...` menu of the page, after its actions. */
  menu?: ReactNode;
  /** The page's one primary button, after the Moebius button. */
  primary?: ReactNode;
  back?: ReactNode;
  oneRow?: boolean;
}) {
  const sidebar = useSidebarToggle();
  const inArea = useContext(InSettingsArea);
  if (inArea) {
    return (
      <div className="mb-3 flex min-h-10 flex-wrap items-center gap-x-4 gap-y-1">
        <h1 className="m-0 min-w-0 truncate text-xl font-semibold tracking-tight text-text">
          {title}
        </h1>
        {tabs && <div className="flex min-w-0">{tabs}</div>}
        {(actions || menu || primary) && (
          <div className="ml-auto flex min-w-0 max-w-full flex-wrap items-center gap-2">
            {actions}
            {menu}
            {primary}
          </div>
        )}
      </div>
    );
  }
  return (
    <header
      data-page-header
      data-tauri-drag-region
      style={{ containerType: "inline-size", containerName: "header" }}
      className={cn(
        "sticky -top-4 z-10 -mx-4 -mt-4 mb-6 flex min-h-12 shrink-0 items-center gap-x-4 gap-y-1 py-1 border-b border-hairline bg-panel pl-4 pr-[calc(1rem+var(--window-buttons))]",
        oneRow ? "h-12 flex-nowrap" : "flex-wrap",
      )}
    >
      <div className="flex min-w-0 items-center gap-1">
        {sidebar?.narrow && (
          <IconButton
            icon={SidebarIcon}
            iconWeight="regular"
            iconClassName="size-5"
            aria-label="Show the sidebar"
            title="Show the sidebar"
            className="-ml-2"
            onClick={() => sidebar.setOpen(true)}
          />
        )}
        {back}
        <h1 className="m-0 min-w-0 truncate text-md font-semibold tracking-tight text-text">
          {title}
        </h1>
      </div>
      {tabs && (
        <div
          className={cn(
            "-mb-1 flex self-stretch [&>[role=tablist]]:items-stretch [&_[role=tab]]:h-full [&_[role=tab]]:pb-1",
            // One row: the tabs keep theirs and the title gives way (it is cut with an ellipsis), so a tab never
            // wraps below the header.
            oneRow ? "shrink-0 [&>[role=tablist]]:flex-nowrap [&>[role=tablist]]:gap-3" : "min-w-0",
          )}
        >
          {tabs}
        </div>
      )}
      <div
        className={cn(
          "ml-auto flex max-w-full items-center gap-2",
          oneRow ? "shrink-0 flex-nowrap" : "min-w-0 flex-wrap",
        )}
      >
        {actions}
        {menu}
        <AgentButton />
        {primary}
      </div>
    </header>
  );
}

/** A sentence of a box. */
export function Body({ children }: { children: ReactNode }) {
  return <p className="m-0 max-w-prose text-md text-text">{children}</p>;
}
